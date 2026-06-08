# Permissions & udev

By default `/dev/ttyACM*` and `/dev/video*` are not accessible to a normal user.
openterface-rs ships a **least-privilege** udev rule
([`packaging/udev/60-openterface.rules`](../../packaging/udev/60-openterface.rules))
that grants access two ways.

## The access model

1. **Seat / `uaccess` (desktop).** The rule tags the Openterface nodes with
   `uaccess`, so the user logged in on the local seat gets an ACL automatically
   from `systemd-logind`. No group membership is needed. This is the right model
   for a normal desktop session.

2. **Group fallback (headless / SSH / VM).** When there is no active local seat
   (e.g. you SSH into a box), `uaccess` does not apply. The rule also sets the
   node's group to a dedicated **`openterface`** group with mode `0660`. Add
   yourself to it:

   ```bash
   sudo usermod -aG openterface "$USER"
   # then log out and back in (or: newgrp openterface) for it to take effect
   ```

This is deliberately **not** `MODE="0666"` (world access) and **not** the broad
`dialout`/`video` groups: input injection is a trust boundary (see
[`SECURITY.md`](../../SECURITY.md)).

## Installing the rules

`install.sh` does this for you (and creates the `openterface` group). Manually:

```bash
sudo install -m 0644 packaging/udev/60-openterface.rules /etc/udev/rules.d/
sudo groupadd --system openterface 2>/dev/null || true
sudo udevadm control --reload
sudo udevadm trigger
```

udev rules apply to devices added **after** they load, so **unplug and replug**
the Openterface (or reboot) after installing.

## Verifying

```bash
# Find the serial node and confirm your user can read/write it.
openterface-rs scan
ls -l /dev/ttyACM0
getfacl /dev/ttyACM0     # on a desktop seat you should see your user via uaccess
```

## NixOS

Reference the rules from the package in your configuration:

```nix
services.udev.packages = [ pkgs.openterface-rs ];
users.users.<you>.extraGroups = [ "openterface" ];
users.groups.openterface = { };
```

## Common permission errors

- **`Permission denied` opening `/dev/ttyACM0` or `/dev/videoN`** — rules not
  installed, device plugged in before the rules loaded (replug), or (headless)
  you are not in the `openterface` group / have not re-logged in.
- **`Device or resource busy`** — another process holds the node (a previous
  `openterface-rs`, ModemManager probing the serial port, or another capture
  app). Stop it and retry; consider masking ModemManager for the CH9329.
