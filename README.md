# nm-dbus-proxy-rs

D-Bus service that implements the [NetworkManager][1] D-Bus API, but interacting with non-NetworkManager components underneath

## what? why?

- [systemd][2] has become the dominant init system for desktop Linux
- [NetworkManager][1] is a popular/dominant component for managing network devices and connections on desktop Linux
- the [systemd][2] suite does include a similar solution to [NetworkManager][1]: [systemd-networkd][3]
- as [systemd-networkd][3] is less prevalent/popular, many Linux desktop components do not integrate with it
- `nm-dbus-proxy` is an attempt to bridge the gap between such components and [systemd-networkd][3]

## TODO

- [ ] every read-only NetworkManager API mapped to [systemd-networkd][3] underneath
- [ ] every read+write NetworkManager API mapped to [systemd-networkd][3] underneath

## see also

- [1]: https://www.networkmanager.dev/
- [2]: https://systemd.io/
- [3]: https://www.freedesktop.org/software/systemd/man/latest/systemd-networkd.html
