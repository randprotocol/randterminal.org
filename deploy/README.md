# Deploying randterminal.org

One single-purpose droplet: 4 vCPU / 8 GB / 160 GB minimum (two concurrent 4 GB sandboxes; SPL Token's
emulation peaks around 1 GB; the data cap is 20 GB). Membership of the `docker` group is root-equivalent,
so hold no other secrets on it.

1. Provision with `cloud-init.yaml` (user data on first boot).
2. Copy `api.env.example` to `/etc/terminal/api.env` (mode 600, owner `terminal`); `TERMINAL_SANDBOX_IMAGE`
   must be set to the tag `sandbox/tag.sh` prints (`terminal-sandbox:<circuits rev>-<hash of sandbox/>`), which
   `deploy.sh` warns about if it does not; the api's built-in default is for development only.
3. Install `terminal-api.service` and `terminal-web.service` in `/etc/systemd/system/` and `systemctl enable` both.
4. Point the `randterminal.org` and `www` A records at the droplet. `nginx.conf` cannot load before the
   certificate exists, so first install `nginx-bootstrap.conf` as `/etc/nginx/sites-enabled/randterminal.org`
   (remove the default site), `systemctl reload nginx`, and run
   `mkdir -p /var/www/certbot && certbot certonly --webroot -w /var/www/certbot -d randterminal.org -d www.randterminal.org --deploy-hook "systemctl reload nginx"`
   (or `--standalone` with nginx stopped). Then replace the site file with `nginx.conf` and reload. If Cloudflare
   proxies the domain, enable the `set_real_ip_from` / `real_ip_header CF-Connecting-IP` block at the top of
   `nginx.conf`.
5. `deploy/deploy.sh <ip>` from a clean checkout with the submodule. The first deploy builds the sandbox image
   (30-40 minutes on a 4 vCPU droplet); later deploys rebuild it only when the circuits revision or anything under `sandbox/` changes.
   It ends with the api health check and a web check on the box; then
   `curl -fsS https://randterminal.org/api/v1/health`.

nginx sets `X-Forwarded-For` to the single `$remote_addr` hop; the api (`TRUST_PROXY=true`) rate-limits on the
last hop.
