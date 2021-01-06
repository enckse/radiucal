#!/bin/bash
LIBRARY=/var/lib/radiucal
SETUP=$LIBRARY/setup.log
ENV=/etc/radiucal/env
COMMIT=$LIBRARY/commit
PREV=$COMMIT.prev
RADIUCAL_REPO=/var/cache/authem/repo

_random-string() {
    cat /dev/urandom | tr -dc 'a-zA-Z0-9' | fold -w ${1:-32} | head -n 1
}

_init() {
    local pass cwd
    echo "setting up hostapd/radiucal"
    pass=$(_random-string)
    sed -i "s/{PASSWORD}/$pass/g" /etc/radiucal/hostapd/certs/*.cnf /etc/radiucal/hostapd/hostapd.conf
    cwd=$PWD
    cd /etc/radiucal/hostapd/certs/ && ./bootstrap
    cd $cwd
}


if [ ! -e $SETUP ]; then
    echo "performing first-time grad setup"
    _init >> $SETUP 2>&1
fi

_hostapd() {
    /usr/lib/radiucal/hostapd /etc/radiucal/hostapd/hostapd.conf | sed "s/^/[hostapd] /g"
}

_radiucal() {
    /usr/bin/radiucal --config /etc/radiucal/$1.conf --instance $1 | sed "s/^/[radiucal-$1] /g"
}

while [ 1 -eq 1 ]; do
    git -C $RADIUCAL_REPOSITORY pull > /dev/null 2>&1
    git -C $RADIUCAL_REPOSITORY log -n1 --format=%h > $COMMIT
    if [ -e $PREV ]; then
        diff -u $PREV $COMMIT
        if [ $? -ne 0 ]; then
            echo "configuration change detected"
            for p in $(pidof hostapd); do
                kill -HUP $p
            done
            for p in $(pidof radiucal-runner); do
                kill -2 $p
            done
        fi
    fi
    mv $COMMIT $PREV
    if ! pgrep '^hostapd$' > /dev/null; then
        echo "starting hostapd"
        _hostapd &
    fi
    if ! pgrep '^radiucal$' > /dev/null; then
        echo "starting radiucal"
        _radiucal proxy &
        _radiucal accounting &
    fi
    sleep 5;
done
