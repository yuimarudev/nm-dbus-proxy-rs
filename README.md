# nm-dbus-proxy-rs

D-Bus service that implements the [NetworkManager][1] D-Bus API, but interacting with non-NetworkManager components underneath

## upstream and licensing

This repository is a modified fork of [Ron Waldon-Howe / nm-dbus-proxy-rs][6].
The original upstream work is copyright Ron Waldon-Howe.
This fork remains distributed under the Apache License 2.0; see [LICENSE](./LICENSE).

## what? why?

- [systemd][2] has become the dominant init system for desktop Linux
- [NetworkManager][1] is a popular/dominant component for managing network devices and connections on desktop Linux
- the [systemd][2] suite does include a similar solution to [NetworkManager][1]: [systemd-networkd][3]
- as [systemd-networkd][3] is less prevalent/popular, many Linux desktop components do not integrate with it
- `nm-dbus-proxy` is an attempt to bridge the gap between such components and [systemd-networkd][3]

## installation

1. `cargo build --release` and copy target/release/nm-dbus-proxy to /usr/bin/
1. copy nm-dbus-proxy.service to /usr/lib/systemd/system/
1. `systemctl daemon-reload` and `systemctl enable nm-dbus-proxy.service`
1. copy org.freedesktop.NetworkManager.conf to /usr/share/dbus-1/system.d/
1. copy org.freedesktop.NetworkManager.service to /usr/share/dbus-1/system-services/
1. reboot

## TODO

- [x] automatically activate on-demand (when "installation" process is completed)
- [ ] map enough NetworkManager to system-networkd for [cosmic-applets][4] to display one-off network status
- [ ] map enough NetworkManager to system-networkd for [cosmic-greeter][5] to display one-off network status
- [ ] map enough NetworkManager to system-networkd for [cosmic-applets][4] to display live network status
- [ ] map enough NetworkManager to system-networkd for [cosmic-greeter][5] to display live network status
- [ ] map enough NetworkManager to system-networkd for [cosmic-applets][4] to toggle airplane mode
- [ ] map enough NetworkManager to system-networkd for [cosmic-applets][4] to toggle Wi-Fi
- [ ] improve the installation process, i.e. provide scripts and/or distribution packages
- [ ] every read-only NetworkManager API mapped to [systemd-networkd][3] underneath
- [ ] every read+write NetworkManager API mapped to [systemd-networkd][3] underneath

[1]: https://www.networkmanager.dev/
[2]: https://systemd.io/
[3]: https://www.freedesktop.org/software/systemd/man/latest/systemd-networkd.html
[4]: https://github.com/pop-os/cosmic-applets
[5]: https://github.com/pop-os/cosmic-greeter
[6]: https://gitlab.com/jokeyrhyme/nm-dbus-proxy-rs
