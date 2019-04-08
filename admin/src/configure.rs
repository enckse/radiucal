use crate::constants::{CONFIG_DIR, EAP_USERS, IS_YAML, MANIFEST, OUTPUT_DIR, PASSWORDS};
use std::fs::{copy, create_dir, create_dir_all, remove_file, write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;
extern crate chrono;
use crate::objects::{
    load_objects, load_passwords, load_users, load_vlans, Object, Password, User, VLAN,
};
use chrono::Local;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::prelude::*;

const HASHED: &str = "last";
const MAB_MODE: &str = "mab";
const LOGIN_MODE: &str = "login";
const OWN_MODE: &str = "owned";

fn kill(pid: &str, signal: &str) -> bool {
    let output = Command::new("kill")
        .arg(signal)
        .arg(pid)
        .status()
        .expect("kill command failed");
    return output.success();
}

fn signal(name: &str, signal: &str) -> bool {
    let output = Command::new("pidof")
        .arg(name)
        .output()
        .expect("pidof command failed");
    let s = match str::from_utf8(&output.stdout) {
        Ok(v) => v,
        Err(e) => {
            println!("signal failed: {}", name);
            println!("{}", e);
            return false;
        }
    };
    let parts: Vec<&str> = s.split_whitespace().collect();
    let mut valid = true;
    for p in parts {
        if !kill(p, &format!("-{}", signal)) {
            valid = false;
        }
    }
    return valid;
}

fn signal_all() -> bool {
    if !signal("hostapd", "HUP") {
        return false;
    }
    if !signal("radiucal", "2") {
        return false;
    }
    return true;
}

fn update(outdir: PathBuf) -> bool {
    let manifest = outdir.join(MANIFEST);
    if !manifest.exists() {
        println!("missing manifest file");
        return false;
    }
    let var_lib = Path::new("/tmp/var/lib/radiucal/");
    let var_home = var_lib.join("users");
    if !var_home.exists() {
        create_dir_all(&var_home).expect("unable to make live configs");
    }
    let contents = fs::read_to_string(manifest).expect("unable to read manifest");
    let base_users: std::vec::Vec<&str> = contents.split("\n").collect();
    let mut new_users: std::vec::Vec<String> = std::vec::Vec::new();
    for b in base_users {
        if b == "" {
            continue;
        }
        new_users.push(var_home.join(b).to_string_lossy().into_owned());
    }
    let cur_users = get_file_list(&var_home.to_string_lossy().into_owned());
    for u in cur_users {
        match new_users.iter().position(|r| r == &u) {
            Some(_) => {}
            None => {
                println!("dropping file {}", u);
                remove_file(u).expect("unable to remove file");
            }
        }
    }
    for u in new_users {
        if u == "" {
            continue;
        }
        let user_file = Path::new(&u);
        if !user_file.exists() {
            fs::write(user_file, "user").expect("unable to write file");
        }
    }
    let eap_bin = outdir.join(EAP_USERS);
    let eap_var = var_lib.join(EAP_USERS);
    if !eap_bin.exists() {
        println!("eap_users file is missing?");
        return false;
    }
    copy(eap_bin, eap_var).expect("unable to copy eap_users file");
    return signal_all();
}

fn get_file_list(dir: &str) -> std::vec::Vec<String> {
    let mut file_list: std::vec::Vec<String> = std::vec::Vec::new();
    let files = fs::read_dir(dir).expect("unable to read directory");
    for f in files {
        let entry = f.expect("unable to read dir");
        file_list.push(entry.path().to_string_lossy().into_owned());
    }
    return file_list;
}

fn create_vlan_outputs(vlans: HashMap<String, VLAN>) {
    let out = Path::new(OUTPUT_DIR);
    let mut dot =
        File::create(out.join("segment-diagram.dot")).expect("unable to create dot diagram");
    let mut md = File::create(out.join("segments.md")).expect("unable to create segments markdown");
    dot.write(
        b"digraph g {
    size=\"6,6\";
    node [color=lightblue2, style=filled];
",
    )
    .expect("dot header failed");
    md.write(
        b"| cell | segment | lan | vlan | owner | description |
| --- | --- | --- | --- | --- | --- |
",
    )
    .expect("md header failed");
    let mut vlan_keys: Vec<&str> = vlans.iter().map(|(a, _)| (&a[..])).collect::<Vec<_>>();
    vlan_keys.sort();
    for v in vlan_keys {
        let obj = vlans.get(v).expect("unable to internally index vlans");
        dot.write(obj.to_diagram().as_bytes())
            .expect("could not append to dot file");
        md.write(obj.to_markdown().as_bytes())
            .expect("could not append to md file");
    }
    dot.write(b"}\n").expect("unable to close dot file");
}

struct Audit {
    user: String,
    vlan: String,
    mac: String,
}

struct Whitelist {
    user: String,
    mac: String,
}

struct Eap {
    user: String,
    pass: String,
    vlan: i32,
    md5: bool,
}

struct SysInfo {
    id: String,
    make: String,
    model: String,
    obj_type: String,
    system_type: String,
    user: String,
}

struct Manifest {
    audit: Vec<Audit>,
    whitelist: Vec<Whitelist>,
    eap_users: Vec<Eap>,
    sys_info: Vec<SysInfo>,
}

impl Manifest {
    fn audits(&self, out: &Path) {
        let mut v = Vec::new();
        for a in &self.audit {
            v.push(format!("{},{},{}\n", a.user, a.vlan, a.mac));
        }
        v.sort();
        let mut audits = File::create(out.join("audit.csv")).expect("unable to create audit csv");
        for a in v {
            audits
                .write(a.as_bytes())
                .expect("unable to write audit entry");
        }
    }
}

fn check_objects(
    vlans: &HashMap<String, VLAN>,
    objects: &HashMap<String, Object>,
    users: &HashMap<String, User>,
    passes: &HashMap<String, Password>,
) -> Option<Manifest> {
    let mut manifest = Manifest {
        audit: Vec::new(),
        whitelist: Vec::new(),
        eap_users: Vec::new(),
        sys_info: Vec::new(),
    };
    for v in vlans.keys() {
        let vlan_obj = &vlans[v];
        for i in &vlan_obj.initiate {
            if !vlans.contains_key(i) {
                println!("{} has initiate that is not a vlan: {}", v, i);
                return None;
            }
        }
    }
    let mut tracked_macs: HashMap<String, String> = HashMap::new();
    for u in users.keys() {
        let user_obj = &users[u];
        if !vlans.contains_key(&user_obj.default_vlan) {
            println!("{} has invalid default vlan: {}", u, user_obj.default_vlan);
            return None;
        }
        if !passes.contains_key(u) {
            println!("{} has no password", u);
            return None;
        }
        for d in &user_obj.devices {
            if !objects.contains_key(&d.base) {
                println!("{} -> {} has invalid base: {}", u, d.name, d.base);
                return None;
            }
            for mac in d.macs.keys() {
                let a = &d.macs[mac];
                let mut audit_vlan = String::new();
                match a.mode.as_str() {
                    MAB_MODE => {
                        audit_vlan.push_str(&a.vlan);
                    }
                    LOGIN_MODE => {
                        audit_vlan.push_str(&a.vlan);
                    }
                    OWN_MODE => {
                        audit_vlan = "n/a".to_string();
                    }
                    _ => {
                        println!("unknown mode for {} -> {}", u, mac);
                        return None;
                    }
                }
                let mut audit = Audit {
                    user: u.to_string(),
                    mac: mac.to_string(),
                    vlan: audit_vlan.to_string(),
                };
                manifest.audit.push(audit);
                match tracked_macs.get(mac) {
                    Some(v) => {
                        if &a.mode != v {
                            println!("{} -> {} cannot change type (owned, mab or login)", u, mac);
                            return None;
                        }
                    }
                    None => {
                        tracked_macs.insert(mac.to_string(), a.mode.to_owned());
                    }
                }
            }
        }
    }
    return Some(manifest);
}

pub fn netconf() -> bool {
    let configs = fs::read_dir(CONFIG_DIR).expect("unable to read config dir");
    let mut paths: Vec<PathBuf> = Vec::new();
    for p in configs {
        match p {
            Ok(path) => {
                let path_raw = path.path();
                if path_raw.to_string_lossy().ends_with(IS_YAML) {
                    paths.push(path_raw);
                }
            }
            Err(e) => {
                println!("unable to read config dir file {}", e);
                return false;
            }
        }
    }
    let mut vlan_args: Vec<String> = Vec::new();
    let vlans = match load_vlans(&paths) {
        Ok(v) => v,
        Err(e) => {
            println!("{}", e);
            return false;
        }
    };
    for v in vlans.keys() {
        vlan_args.push(format!(
            "{}={}",
            v,
            vlans.get(v).expect("internal map error").number
        ));
    }
    if vlans.len() == 0 {
        println!("no vlans defined");
        return false;
    }
    let objs = match load_objects(
        Path::new(CONFIG_DIR)
            .join("objects.yaml")
            .to_string_lossy()
            .to_string(),
    ) {
        Ok(o) => o,
        Err(e) => {
            println!("{}", e);
            return false;
        }
    };
    let all_users = match load_users(&paths) {
        Ok(u) => u,
        Err(e) => {
            println!("{}", e);
            return false;
        }
    };
    let user_passes = match load_passwords(
        Path::new(CONFIG_DIR)
            .join(PASSWORDS)
            .to_string_lossy()
            .to_string(),
    ) {
        Ok(o) => o,
        Err(e) => {
            println!("{}", e);
            return false;
        }
    };
    match check_objects(&vlans, &objs, &all_users, &user_passes) {
        Some(m) => {
            let out = Path::new(OUTPUT_DIR);
            m.audits(out);
        }
        None => {
            return false;
        }
    }
    create_vlan_outputs(vlans);
    let output = Command::new("radiucal-admin-legacy")
        .args(vlan_args)
        .status()
        .expect("legacy command failed");
    return output.success();
}

pub fn all(client: bool) -> bool {
    println!("updating networking configuration");
    let outdir = Path::new(OUTPUT_DIR);
    if !outdir.exists() {
        create_dir(outdir).expect("unable to make output directory");
    }
    let hash = outdir.join(HASHED);
    let prev_hash = outdir.join(HASHED.to_owned() + ".prev");
    if hash.exists() {
        copy(&hash, &prev_hash).expect("unable to maintain last hash");
    }
    if !netconf() {
        return false;
    }
    let server = !client;
    let date = Local::now().format(".radius.%Y-%m-%d").to_string();
    if server {
        println!("checking for daily operations");
        let daily = Path::new("/tmp/").join(date);
        if !daily.exists() {
            println!("running daily operations");
            if !signal_all() {
                println!("failed signaling daily");
            }
            match write(daily, "done") {
                Ok(_) => {}
                Err(e) => {
                    println!("unable to write daily indicator {}", e);
                }
            }
        }
    }

    let mut diffed = true;
    let mut file_list = get_file_list(CONFIG_DIR);
    file_list.sort();
    let output = Command::new("sha256sum")
        .args(file_list)
        .output()
        .expect("sha256sum command failed");
    fs::write(&hash, output.stdout).expect("unable to store hashes");
    if hash.exists() && prev_hash.exists() {
        let output = Command::new("diff")
            .arg("-u")
            .arg(hash)
            .arg(prev_hash)
            .status()
            .expect("diff command failed");
        diffed = !output.success();
    }
    if diffed {
        println!("configuration updated");
        if server {
            return update(outdir.to_path_buf());
        }
    }
    return true;
}
