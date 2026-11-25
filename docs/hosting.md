# Hosting glitchRx for other people

How to put the web GUI where other people can use it: first a close
circle, later perhaps the general public. Written 2026-11-23 against the
code as it is on `main`; the "must build first" lists are work that has not
been done yet.

For running it for yourself, in a container of your own with your photo
library mounted, see [section 8](#8-personal-use-your-own-container-from-github).

## 1. Where it stands today

`glitchrx serve` is built for one trusted person on their own network:

- **One shared token.** `--token` is a single password for everyone; there
  are no accounts and no per-person sessions.
- **It can reach the server's disk.** Several endpoints read or write
  files on the machine it runs on, by design, for Phil's own library:
  - read: `/api/browse`, `/api/thumb`, `/api/folders`, `/api/docs/open`,
    `/api/recipes`, `/api/recipe`, `/api/settings`
  - write: `/api/settings` (PUT, which also chooses the library folder,
    so a visitor could point it anywhere the process can read),
    `/api/docs/{id}/save`, `save-to`, `save-as`, `/api/presets` (PUT,
    DELETE)
  - the desktop dialogs (`/api/desktop/open`, `save-as`) only exist in
    the desktop app.
- **Open photos are shared.** The server keeps the 4 most recently used
  photos for everyone together. With several visitors they would push
  each other's photos out. Photo ids are 128-bit random, so nobody can
  guess someone else's, but nothing ties a photo to the person who
  uploaded it either.
- **Limits that exist:** uploads up to 256 MB; live previews give up after
  3 seconds; preview images at most 4096 px a side.
- **Limits that do not exist yet:** no cap on a picture's pixel count (a
  small file can claim 60,000 x 60,000 pixels and make the server try to
  allocate 10 GB: a "decompression bomb"); no limit on renders running at
  once or per visitor; no time limit on exports.

Good news for privacy, already true:

- Uploaded photos live only in memory. Nothing is written to disk unless
  someone uses the save endpoints above, and they are switched off in the
  public mode described below.
- Exports keep the colour profile but drop EXIF, so no GPS position,
  camera serial or date leaves with the picture.
- No network calls at runtime, no analytics, no external fonts or scripts.

## 2. Stage 1: a close circle (soon, small changes)

For a handful of friends who you trust not to poke at it:

1. **A dedicated container or VM.** An unprivileged LXC container (or a
   small VM) on your server that holds nothing personal: no bind mounts
   of your photo library, no shared home folder. If someone did reach the
   disk endpoints, all they would find is an empty container.
2. **Run it as an unprivileged service,** bound to localhost only, so the
   tunnel is the only way in:

       glitchrx serve --host 127.0.0.1 --port 7878 --root /srv/glitch-empty

   `--root` on an empty folder fixes the library there; Settings cannot
   move it.
3. **Cloudflare Tunnel** (`cloudflared`) inside the container, pointing a
   hostname at `http://127.0.0.1:7878`. See section 4.
4. **Cloudflare Access in front** (Zero Trust, free for up to 50 users): a
   policy that lets in only your friends' email addresses, who sign in
   with a one-time code sent by email. Nobody else ever reaches the app,
   so the open endpoints above are not exposed to the internet at large.
5. **Firewall the container off from your LAN.** It needs outbound
   internet (for the tunnel) and nothing else: no route to your NAS,
   router admin page or other machines.
6. Tell your friends the 4-photos-at-once limit: if several use it at the
   same moment, an idle photo may need opening again.

## 3. Stage 2: the general public (needs building first)

### Must build first: a public mode

A `--public` switch (or a separate small binary) that:

- **Removes every disk endpoint** listed in section 1, and the presets
  editor; built-in presets could stay, read-only and compiled in.
- **Gives each visitor their own session:** a random, HTTP-only, SameSite
  cookie; photos belong to the session that uploaded them, and requests for
  another session's photo get 404.
- **Caps memory:** at most 1 or 2 open photos per session, a server-wide
  memory budget (for example 2 GB) with the least recently used photos
  closed first, and a "server is busy" message instead of running out.
- **Deletes idle photos:** after 15 to 30 minutes without a request, and
  when the visitor leaves (a `navigator.sendBeacon` on page close). The
  memory is simply dropped; nothing was ever on disk.
- **Caps picture size:** refuse anything over a pixel budget (say 50
  megapixels) from the header, before decoding.
- **Limits work:** renders at once no more than the CPU cores, at most one
  running per session, and a time limit on exports (under the tunnel's
  100 seconds, see section 4).
- **Logs nothing personal:** no file names, no IP addresses in the app's
  own logs (Cloudflare has its own).
- **Sends security headers:** a strict Content Security Policy (the GUI
  already loads nothing from other sites), `frame-ancestors 'none'`,
  `Referrer-Policy: no-referrer`. `nosniff` is already sent.
- **Has a short privacy note** in the GUI: photos stay in memory, are
  deleted after N minutes, are never stored or looked at.

Effort: roughly a phase of its own (about the size of Phase 5e plus
tests), mostly in `glitchrx-serve`.

### Also worth doing

- **Rate limits at the edge.** A Cloudflare rate-limiting rule on
  `POST /api/docs` (uploads), for example 10 a minute per IP.
- **Bot protection.** Cloudflare Turnstile on the first upload, if the
  site gets found by scrapers.
- **A terms and privacy page,** even a short one, once strangers use it.
- **Updates.** Rebuild and restart when dependencies get security fixes
  (`cargo audit` in the gate would flag them).

## 4. Cloudflare Tunnel: yes, with three limits to know

A tunnel suits this well. `cloudflared` makes an outbound connection to
Cloudflare, so no port is opened on your router, your home IP is not
published, and Cloudflare absorbs attacks before they reach you.

Limits that matter for this app:

1. **Upload size: 100 MB per request** on the Free and Pro plans. Camera
   JPEGs (10 to 40 MB) are fine; a large uncompressed TIFF may be refused
   with error 413. Either cap uploads at 100 MB in public mode with a clear
   message, or tell people to use JPEG.
2. **Response time: 100 seconds.** A request that takes longer ends in
   error 524. Previews are far faster; a heavy export of a very large
   photo is the only risk, which the export time limit above covers.
3. **Terms of service.** Cloudflare's terms restrict using its network
   mainly to serve large amounts of video or other big media. A small
   image tool should be fine; read the current terms before a public
   launch, and keep in mind that images here are generated on request,
   not cached.

Setup, inside the container:

    # Install cloudflared (Debian/Ubuntu package from Cloudflare's repo),
    # then, logged into your Cloudflare account:
    cloudflared tunnel login
    cloudflared tunnel create glitch
    cloudflared tunnel route dns glitch glitch.example.com
    # /etc/cloudflared/config.yml:
    #   tunnel: glitch
    #   credentials-file: /etc/cloudflared/<id>.json
    #   ingress:
    #     - hostname: glitch.example.com
    #       service: http://127.0.0.1:7878
    #     - service: http_status:404
    cloudflared service install

Alternatively, create the tunnel from the Zero Trust dashboard and paste
its one-line install command; that also makes adding Access (stage 1)
point-and-click.

## 5. Hardware

Measured on a 12-megapixel (4000 x 3000) test picture, release build, on
a 16-thread desktop:

| | Time | Memory |
|---|---|---|
| Server at rest | | about 120 MB |
| One open photo, previews and an exact render | | about 100 MB more |
| Live preview of a 3-step recipe (phone screen size) | 5 to 60 ms | |
| Exact full-size render of the same | 20 to 400 ms (first one slowest) | |
| JPEG export | about 50 ms after a render | |

Memory grows with the photo: roughly 8 to 9 bytes per pixel while open
(the original, the working canvas, cached steps and renders), so a
24-megapixel photo takes about 200 MB and a 50-megapixel one about 450 MB.
Renders use one core each.

Suggested sizes:

- **Close circle (a few people, now and then):** 2 vCPU, 2 GB RAM,
  a few GB of disk for the system. A spare corner of a home server is
  plenty.
- **Small public site (tens of people at once):** 4 vCPU, 4 to 8 GB RAM,
  with the public mode's memory budget set to about half the RAM.
- **Busier:** CPU becomes the limit before memory does. Scale up the
  container's cores, or run several containers behind one tunnel
  (Cloudflare balances them), each with its own sessions.

Disk is never the issue: nothing is stored.

## 6. Locking down the container

For the public or friends container of sections 3 and 4, which holds no
personal files. For your own container with your photos, use the unit in
section 8.4 instead: this one cannot see any home folder.

- **Unprivileged LXC** (or a VM if you want a stronger wall), with only
  the binary and `cloudflared` installed.
- **A systemd unit with hardening** for glitchrx:

      [Service]
      ExecStart=/usr/local/bin/glitchrx serve --host 127.0.0.1 --port 7878 --root /srv/glitch-empty
      DynamicUser=yes
      ProtectSystem=strict
      ProtectHome=yes
      PrivateTmp=yes
      PrivateDevices=yes
      NoNewPrivileges=yes
      RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
      MemoryMax=3G
      CPUQuota=300%
      Restart=on-failure

  `MemoryMax` makes the kernel stop the app, not the host, if a bug or an
  attack ever eats memory; systemd restarts it.
- **No inbound ports** on the container; the tunnel is outbound.
- **Outbound firewall:** allow only what `cloudflared` needs (Cloudflare's
  network on port 7844, and 443) and DNS; block the LAN ranges.
- **Automatic security updates** for the container's OS
  (`unattended-upgrades` on Debian).

## 7. Checklist

Stage 1 (close circle):

- [ ] Empty unprivileged container, LAN blocked
- [ ] glitchrx as a hardened systemd service on 127.0.0.1, `--root` on an
      empty folder
- [ ] Cloudflare Tunnel to a hostname
- [ ] Cloudflare Access policy with your friends' emails
- [ ] Try it from a phone off your network

Stage 2 (public), after building public mode:

- [ ] Public mode: no disk endpoints, sessions, memory budget, idle
      deletion, pixel cap, render limits, headers, privacy note
- [ ] Upload cap matched to the tunnel's 100 MB
- [ ] Rate-limiting rule on uploads
- [ ] Access policy removed (or kept for a beta)
- [ ] Terms and privacy page
- [ ] Load test: several phones at once, a 50-megapixel photo, a
      decompression bomb (refused)

## 8. Personal use: your own container, from GitHub

Phil's setup (2026-11-24): an Arch Linux LXC container that clones the
code from GitHub, builds it, and runs it as a service for himself over the
LAN and WireGuard, with his photography dataset and a glitch dataset
bind-mounted from ZFS. Unlike section 2, this container holds personal
files on purpose; that is fine because only you reach it. Never put it
behind a tunnel or a forwarded port as it is.

### 8.1 GitHub

Everything so far is committed locally only. Pushing `main` sends its whole
history (every branch was merged into it); the old branch names can stay
local.

1. On GitHub, create an **empty** repository: no README, licence or
   `.gitignore` (the project has its own).
2. On the development machine:

       git remote add origin git@github.com:USER/glitchrx.git
       git push -u origin main
       git push origin archive/jpeg-bytes     # the README points old recipes here

3. In the container, clone it:
   - public repository: `git clone https://github.com/USER/glitchrx.git`
   - private repository: make an SSH key in the container
     (`ssh-keygen -t ed25519`), add the public half to the repository as a
     **read-only deploy key** (Settings, Deploy keys), and clone with the
     `git@github.com:` address.

`samples/` (personal photos) is in `.gitignore` and has never been
committed; nothing in the history needs removing before a push.

### 8.2 Packages (Arch)

    sudo pacman -S --needed base-devel git rust nodejs npm libjpeg-turbo just

Arch's `rust` package is current and updates with `pacman -Syu`, which is
all a server needs. `rust` and `rustup` conflict; if `rust` is already
installed (paru pulls it in), keep it. rustup is only worth it for
switching toolchains or cross-compiling.

Building needs about 2 GB of memory and a few minutes on four cores;
running needs only `libjpeg-turbo`. Give the container 4 GB of RAM and a
few cores.

### 8.3 Folders

The folder pickers in Settings and Save As only reach folders **inside the
home folder of the user running the server**, and refuse symlinks that lead
out of it. So mount both datasets inside that home, under one parent,
rather than at `/mnt` with a symlink:

    /home/phil/media/photography    bind mount, read-only
    /home/phil/media/glitch         bind mount, read-write

Then start the server with `--root /home/phil/media` (Open browses both)
and set the export folder in Settings to `~/media/glitch`. Mounting the
photography dataset read-only is a second guard on top of glitchRx never
writing a photo; the cost is that "Save next to original" cannot write
beside those photos, so save into the glitch folder (Save As, or the
export folder).

On the host, plain LXC (`/var/lib/lxc/NAME/config`):

    lxc.mount.entry = /tank/photography home/phil/media/photography none bind,ro,create=dir 0 0
    lxc.mount.entry = /tank/glitch home/phil/media/glitch none bind,create=dir 0 0

Proxmox:

    pct set ID -mp0 /tank/photography,mp=/home/phil/media/photography,ro=1
    pct set ID -mp1 /tank/glitch,mp=/home/phil/media/glitch

(`/tank/...` stands for your dataset's mountpoint.)

**User ids in an unprivileged container.** The container's uid 1000 is
uid 101000 on the host, so files your host user (uid 1000) owns look like
`nobody` inside, and the container cannot write to the glitch dataset. Two
ways out:

- Simple: on the host, `chown -R 101000:101000 /tank/glitch`. The host user
  then needs group or ACL access to read the results.
- Cleaner: map uid and gid 1000 straight through. In the container's config:

      lxc.idmap = u 0 100000 1000
      lxc.idmap = g 0 100000 1000
      lxc.idmap = u 1000 1000 1
      lxc.idmap = g 1000 1000 1
      lxc.idmap = u 1001 101001 64535
      lxc.idmap = g 1001 101001 64535

  and add `root:1000:1` to both `/etc/subuid` and `/etc/subgid` on the
  host. The container's user `phil` must have uid 1000.

  **Then re-own the home folder,** or you are locked out of it: what the
  container made before (the home folder, the clone) belongs to host uid
  101000, which nothing inside maps to any more, so it shows as `nobody`
  and login says "Could not chdir to home directory". On the host, with
  the container stopped:

      pct stop 120
      zfs get -H -o value mountpoint rpool/data/subvol-120-disk-0
      chown -R 1000:1000 /rpool/data/subvol-120-disk-0/home/phil
      pct start 120

  (Use the mountpoint `zfs get` prints; `120` is the container's id.)

### 8.4 Build and run as a service

    cd ~/glitchrx
    just build-all

A token keeps anyone else on the network out. Keep it in a file rather
than the unit, so the unit can be shared:

    mkdir -p ~/.config/glitchrx
    printf 'GLITCHRX_TOKEN=%s\n' "$(openssl rand -hex 16)" > ~/.config/glitchrx/env
    chmod 600 ~/.config/glitchrx/env

`/etc/systemd/system/glitchrx.service`:

    [Unit]
    Description=glitchRx
    After=network-online.target
    Wants=network-online.target

    [Service]
    User=phil
    EnvironmentFile=/home/phil/.config/glitchrx/env
    ExecStart=/home/phil/glitchrx/target/release/glitchrx serve --host 0.0.0.0 --port 7878 --token=${GLITCHRX_TOKEN} --root /home/phil/media
    Restart=on-failure
    NoNewPrivileges=yes
    PrivateTmp=yes
    PrivateDevices=yes
    ProtectSystem=full
    MemoryMax=3G

    [Install]
    WantedBy=multi-user.target

This is the unit for a personal container. Section 6's is stricter
(`DynamicUser`, `ProtectHome`) because a public container must not see
personal files; here the service runs as you and needs your home folder.
`MemoryMax` makes systemd stop and restart the service, rather than the
container running out of memory. If you cloned somewhere other than
`~/glitchrx`, change the path in `ExecStart` (and in 8.5).

Then `sudo systemctl daemon-reload && sudo systemctl enable --now glitchrx`.
`journalctl -u glitchrx` shows the URLs it printed; the token is the one in
the env file. Open `http://CONTAINER-IP:7878/?token=...` once on each
device (and once in the home-screen app); a cookie remembers it.

Reach it on the LAN or over WireGuard only: no port forwarding, no tunnel.

### 8.5 Updating

`git pull` alone does not update the running program: the GUI is built into
the binary. After a pull, build and restart:

    cd ~/glitchrx && git pull --ff-only && just build-all && sudo systemctl restart glitchrx

Recipes and projects from older versions always open in newer ones. If a
build fails, the old binary keeps running until the restart, so nothing is
lost; fix the build (usually a newer Rust: `sudo pacman -Syu`) and run the
line again.

### 8.6 Backups

- The glitch dataset: ZFS snapshots on the host, as for any dataset.
- Presets and settings live in the container, in `~/.config/glitchrx`
  (`presets/` and `config.toml`). Include the container's root in
  snapshots or backups, or move presets onto the glitch dataset with
  `--presets /home/phil/media/glitch/presets`.

Checklist:

- [ ] Empty repository on GitHub; `main` and the `archive/jpeg-bytes` tag pushed
- [ ] Clone in the container (deploy key if private)
- [ ] Packages, `just build-all`
- [ ] Datasets mounted inside `/home/phil/media`, photography read-only
- [ ] Glitch dataset writable from the container (chown or idmap; with idmap, home re-owned)
- [ ] Token file, service enabled, URL opened once per device
- [ ] Export folder set to `~/media/glitch` in Settings
- [ ] Snapshots for the glitch dataset and the container
