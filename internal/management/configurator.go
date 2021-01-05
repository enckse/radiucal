package management

import (
	"bytes"
	"crypto/sha256"
	"fmt"
	"io/ioutil"
	"os"
	"path/filepath"
	"strings"
	"text/template"

	yaml "gopkg.in/yaml.v2"
	"voidedtech.com/radiucal/internal/core"
)

const (
	manifest = "manifest"
	eap      = "eap_users"
	usersCfg = "config.yaml"
)

var (
	trackedFiles = [...]string{manifest, eap, usersCfg}
)

type (
	// Config is the configurator specific handler
	Config struct {
		Key    string
		Cache  string
		deploy bool
		diffs  bool
	}

	configuratorError struct {
	}
)

func (c *configuratorError) Error() string {
	return "configuration change"
}

func unchanged(cfg *Config, radius *RADIUSConfig, users, rawConfig []byte) (bool, error) {
	if !core.PathExists(TempDir) {
		if err := os.Mkdir(TempDir, 0755); err != nil {
			return false, err
		}
	}
	valid := 0
	manifestBytes := []byte(strings.Join(radius.Manifest, "\n"))
	hostapdBytes := append(radius.Hostapd, []byte("\n")...)
	core.WriteInfo("[overall]")
	for _, f := range trackedFiles {
		path := filepath.Join(TempDir, f)
		if core.PathExists(path) {
			b, err := ioutil.ReadFile(path)
			if err != nil {
				return false, err
			}
			if err := ioutil.WriteFile(path+".prev", b, 0644); err != nil {
				return false, err
			}
			switch f {
			case manifest:
				if core.Compare(b, manifestBytes, cfg.diffs) {
					valid++
				}
			case eap:
				if core.Compare(b, hostapdBytes, cfg.diffs) {
					valid++
				}
			case usersCfg:
				if core.Compare(b, users, cfg.diffs) {
					valid++
				}
			default:
				return false, fmt.Errorf("unknown track file: %s", f)
			}
		}
	}
	for k, v := range map[string][]byte{
		manifest: manifestBytes,
		eap:      hostapdBytes,
		usersCfg: users,
	} {
		paths := []string{TempDir}
		if len(cfg.Cache) > 0 {
			paths = append(paths, cfg.Cache)
		}
		for _, f := range paths {
			p := filepath.Join(f, k)
			data := v
			if f != TempDir && k == usersCfg && cfg.deploy {
				data = rawConfig
			}
			if err := ioutil.WriteFile(p, data, 0644); err != nil {
				return false, err
			}
		}
	}
	return valid == len(trackedFiles), nil
}

func getConfig(f string) (*Config, error) {
	if !core.PathExists(f) {
		k, err := GetKey(true)
		if err != nil {
			return nil, err
		}
		return &Config{
			Key:    k,
			diffs:  true,
			deploy: false,
		}, nil
	}
	c := &Config{}
	b, err := ioutil.ReadFile(f)
	if err != nil {
		return nil, err
	}
	if err := yaml.Unmarshal(b, &c); err != nil {
		return nil, err
	}
	c.deploy = true
	c.diffs = false
	return c, nil
}

func hash(value string) string {
	return fmt.Sprintf("%x", sha256.Sum256([]byte(value)))
}

func configurate(cfg string, scripts []string) error {
	config, err := getConfig(cfg)
	if err != nil {
		return err
	}
	loader := LoadingOptions{
		Key:   config.Key,
		NoKey: config.Key == "",
	}
	vlans, err := loader.LoadVLANs()
	if err != nil {
		return err
	}
	systems, err := loader.LoadSystems()
	if err != nil {
		return err
	}
	secrets, err := loader.LoadSecrets()
	if err != nil {
		return err
	}
	users, radius, err := loader.LoadUsers(vlans, systems, secrets)
	if err != nil {
		return err
	}
	if err := loader.BuildTrust(users); err != nil {
		return err
	}
	merged, err := MergeRADIUS(radius)
	if err != nil {
		return err
	}
	u := UserConfig{
		Users: users,
	}
	b, err := yaml.Marshal(u)
	if err != nil {
		return err
	}
	var postProcess []BashRunner
	if len(scripts) > 0 {
		for _, f := range scripts {
			if len(f) == 0 {
				continue
			}
			var scriptables = ToScriptable(u, vlans, systems)
			scriptBytes, err := ioutil.ReadFile(f)
			if err != nil {
				return err
			}
			tmpl, err := template.New("t").Parse(string(scriptBytes))
			if err != nil {
				return err
			}
			var buffer bytes.Buffer
			if err := tmpl.Execute(&buffer, scriptables); err != nil {
				return err
			}
			postProcess = append(postProcess, BashRunner{buffer.Bytes(), filepath.Base(f)})
		}
	}
	raw, err := yaml.Marshal(u)
	if err != nil {
		return err
	}
	var newUsers []*User
	for _, user := range u.Users {
		user.MD4 = hash(user.MD4)
		newUsers = append(newUsers, user)
	}
	u.Users = newUsers
	b, err = yaml.Marshal(u)
	if err != nil {
		return err
	}
	same, err := unchanged(config, merged, b, raw)
	if err != nil {
		return err
	}
	postScript := false
	if same {
		core.WriteInfoDetail("no changes")
	} else {
		core.WriteInfo("changes detected")
		postScript = true
	}
	if postScript && len(postProcess) > 0 {
		first := true
		for _, post := range postProcess {
			if len(post.Data) > 0 {
				if first {
					core.WriteInfo("[scripts]")
					first = false
				}
				core.WriteInfoDetail(post.Name)
				if err := post.Execute(); err != nil {
					return err
				}
			}
		}
	}
	if !same {
		return &configuratorError{}
	}
	return nil
}

// Configurate processes configuration for actual deployment/production
func Configurate(cfg string, scripts []string) {
	err := configurate(cfg, scripts)
	if err != nil {
		if _, ok := err.(*configuratorError); !ok {
			core.ExitNow("unable to configure", err)
		}
		os.Exit(core.ExitSignal)
	}
}