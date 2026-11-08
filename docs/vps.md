# Airth on a VPS

How to move Airth from the Mac to a Linux server that stays on (decision 106). Written for Ubuntu 24.04 or Debian 12, with a user that has `sudo`. The app stays on the server's loopback: you reach it through Tailscale, never an open port.

## 1. Prepare the server

```bash
sudo apt update && sudo apt install -y build-essential git curl bubblewrap

# Ubuntu 24.04 only: AppArmor stops bubblewrap from creating its sandbox
# unless unprivileged user namespaces are allowed.
echo 'kernel.apparmor_restrict_unprivileged_userns=0' | sudo tee /etc/sysctl.d/60-airth-bwrap.conf
sudo sysctl --system

# Rust, to build Airth and its updates.
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"

# Tailscale, to reach the app from your phone and your Mac.
curl -fsSL https://tailscale.com/install.sh | sh
sudo tailscale up
sudo tailscale set --operator="$USER"   # lets you run `tailscale serve` without sudo
```

Building Airth needs about 2 GB of memory. On a smaller server, add swap first.

## 2. Build Airth

```bash
git clone https://github.com/tomonchain/airth ~/airth
cd ~/airth && cargo build --release
mkdir -p ~/.local/bin && ln -sf ~/airth/target/release/airth ~/.local/bin/airth
airth doctor
```

`airth doctor` must show the command sandbox as `bubblewrap`. Updates follow `main` of this checkout, as on the Mac: the app says "Update ready", and Restart installs it.

## 3. Move your data

Only one Airth may run for a business: two would run every routine twice. **On the Mac**, stop it first:

```bash
airth service uninstall
```

Then copy the Airth home, which holds the databases, settings and secrets, over SSH, which encrypts it:

```bash
rsync -a --exclude update --exclude logs --exclude worktrees ~/.airth/ you@your-server:~/.airth/
```

Each business's folder (Hedgen's, for example) is a git repository: push it from the Mac, and clone it on the server.

```bash
# on the server
git clone <the business folder's remote> ~/hedgen
airth realm root ~/hedgen --realm hedgen
airth realm show --realm hedgen   # check the folder and repositories
```

`airth realm show` also lists a business's other repositories. Their paths are the Mac's: change them with `airth realm repo`.

## 4. Start the service

```bash
airth service install
```

Airth now starts at boot, and again five seconds after any stop. If it asks, run the `sudo loginctl enable-linger` line it prints once: without it, the service stops when you log out.

```bash
airth service status
tail -f ~/.airth/logs/serve.log
```

## 5. Reach it from your phone and your Mac

```bash
tailscale serve --bg 7457            # prints the server's address, like https://your-server.tailXXXX.ts.net
airth phone allow your-server.tailXXXX.ts.net
airth phone pair                     # scan the code with the phone
```

On the Mac, open the same address in the browser.

A new sign-in that comes back to `127.0.0.1` (Google Drive, an MCP server's OAuth) needs a tunnel from the Mac: `ssh -L 7457:127.0.0.1:7457 you@your-server`, then the link `airth ui --no-open` prints on the server. Connections made on the Mac moved with the home and keep working.

## Backups

The service copies the home once a day into `~/.airth/backups/` and keeps the last seven. `airth backup` makes one now. They are on the server's own disk: turn on your host's snapshots, or copy `~/.airth/backups` elsewhere, to survive losing the server.

To restore: stop the service (`systemctl --user stop airth`), copy a backup's contents over `~/.airth`, and start it again (`systemctl --user start airth`).

## What stays on the Mac

Scans are not read on Linux (OCR uses macOS, decision 95). The Mac app's window and menu bar icon do not exist on a server: use the browser.
