package usermac

import (
	"testing"

	"layeh.com/radius"
	"layeh.com/radius/rfc2865"
	"voidedtech.com/radiucal/internal/server"
)

func TestUserMacBasics(t *testing.T) {
	newTestSet(t, "test", "11-22-33-44-55-66", true)
	newTestSet(t, "test", "12-22-33-44-55-66", false)
}

func ErrorIfNotPre(t *testing.T, m *umac, p *server.ClientPacket, message string) {
	err := checkUserMac(p)
	if err == nil {
		if message != "" {
			t.Errorf("expected to fail with: %s", message)
		}
	} else {
		if err.Error() != message {
			t.Errorf("'%s' != '%s'", err.Error(), message)
		}
	}
}

func newTestSet(t *testing.T, user, mac string, valid bool) (*server.ClientPacket, *umac) {
	m := setupUserMac()
	if m.Name() != "usermac" {
		t.Error("invalid/wrong name")
	}
	var secret = []byte("secret")
	p := server.NewClientPacket(nil, nil)
	p.Packet = radius.New(radius.CodeAccessRequest, secret)
	ErrorIfNotPre(t, m, p, "radius: attribute not found")
	if err := rfc2865.UserName_AddString(p.Packet, user); err != nil {
		t.Error("unable to add user name")
	}
	ErrorIfNotPre(t, m, p, "radius: attribute not found")
	if err := rfc2865.CallingStationID_AddString(p.Packet, mac); err != nil {
		t.Error("unable to add calling station")
	}
	if valid {
		ErrorIfNotPre(t, m, p, "")
	}
	if !valid {
		ErrorIfNotPre(t, m, p, "failed preauth: test "+clean(mac))
	}
	return p, m
}

func setupUserMac() *umac {
	file = "./tests/manifest"
	m := &umac{}
	m.load()
	return m
}

func TestUserMacCache(t *testing.T) {
	pg, m := newTestSet(t, "test", "11-22-33-44-55-66", true)
	pb, _ := newTestSet(t, "test", "11-22-33-44-55-68", false)
	first := "failed preauth: test 112233445568"
	ErrorIfNotPre(t, m, pg, "")
	ErrorIfNotPre(t, m, pb, first)
}
