# bmcloud

Rust (axum + SQLite) server for managing bare-metal test lab devices:
REST API + built-in web UI, device state reporting, token auth.

## Local development

Requires: [Rust](https://rustup.rs) (stable).

```sh
cd bmcloud-server
cargo run                # server at http://localhost:3000
cargo run --bin farm     # device simulator (run in a second terminal)
cargo test               # unit + integration tests
```

The SQLite database (`bmcloud.db`) is created automatically on first run.

## Deploy (Ubuntu server behind nginx, HTTPS via Let's Encrypt)

Requires: SSH access to an Ubuntu server with ports 80/443 open,
DNS A records for your domain **and** `www.` pointing to it,
[Ansible](https://docs.ansible.com/ansible/latest/installation_guide/introduction.html) installed locally.

1. Fill in `ansible/inventory.ini` (host, user, domain, e-mail) - the file
   is git-ignored, so secrets/hosts stay local
   
2. Run:

   ```sh
   cd ansible
   ansible-playbook deploy.yml
   ```

The playbook builds the project on the server, sets up a `bmcloud`
systemd service, configures an nginx site and issues a TLS certificate
(HTTP redirects to HTTPS, auto-renewal included). Existing nginx sites
on the server are not touched.

Useful commands on the server:

```sh
systemctl status bmcloud      # service state
journalctl -u bmcloud -f      # logs
```

## Layout

```
bmcloud-server/   # axum server (src/), web UI (static/), tests (tests/)
ansible/          # deployment: playbook + templates (inventory.ini is local-only)
```
