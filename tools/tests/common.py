"""Common testing definitions."""
VALID_MAC = "001122334455"

def ready(obj):
    if obj.group == "drop":
        obj.disabled = True
    return obj
