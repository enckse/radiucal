"""User with attributes."""
import netconf as __config__
normal = __config__.Assignment()
normal.macs = ["001122334455"]
normal.vlan = "dev"
normal.management = normal.macs[0]
normal.password = 'e08bb5212dd623b9a1cd4d25bcc8b89r'
