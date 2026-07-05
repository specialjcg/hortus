#!/bin/bash
#
# Script de deploiement Hortus
# Cross-compilation Docker + frontend Elm + deploiement sur VPS
#

set -e

# ============================================================================
# CONFIGURATION
# ============================================================================

VPS_USER="root"
VPS_HOST="VPS-779132.ssh.vps1euro.fr"
VPS_PORT="9999"
VPS_APP_DIR="/opt/hortus"

DOMAIN="hortus.eigenplay.com"

# Port backend (fouduvolant=3000, hortus=3001)
APP_PORT="3001"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR"

# ============================================================================
# COULEURS
# ============================================================================
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# ============================================================================
# VERIFICATIONS
# ============================================================================

check_config() {
    if [ -z "$VPS_HOST" ]; then
        log_error "Configurez VPS_HOST dans le script"
    fi
}

check_dependencies() {
    log_info "Verification des dependances locales..."
    command -v ssh >/dev/null 2>&1 || log_error "ssh non installe"
    command -v scp >/dev/null 2>&1 || log_error "scp non installe"
    command -v docker >/dev/null 2>&1 || log_error "docker non installe (necessaire pour build-docker.sh)"
    log_success "Dependances OK"
}

# ============================================================================
# BUILD LOCAL
# ============================================================================

build_backend() {
    log_info "Compilation du backend Rust (release, via Docker Ubuntu 22.04)..."
    cd "$PROJECT_DIR"
    bash build-docker.sh

    if [ ! -f "backend/target/release/hortus-backend" ]; then
        log_error "Echec de la compilation backend"
    fi

    BINARY_SIZE=$(du -h backend/target/release/hortus-backend | cut -f1)
    log_success "Backend compile ($BINARY_SIZE)"
}

build_frontend() {
    log_info "Compilation du frontend Elm..."
    cd "$PROJECT_DIR/frontend"

    if [ ! -x "node_modules/.bin/elm" ]; then
        log_info "Installation des devDependencies npm..."
        npm install --silent
    fi

    ./node_modules/.bin/elm make src/Main.elm --optimize --output=static/elm.js

    if [ ! -f "static/elm.js" ]; then
        log_error "Echec de la compilation frontend"
    fi

    cd "$PROJECT_DIR"
    log_success "Frontend compile"
}

# ============================================================================
# PACKAGING
# ============================================================================

create_package() {
    log_info "Creation du package de deploiement..."
    cd "$PROJECT_DIR"

    PACKAGE_DIR="deploy_package"
    rm -rf "$PACKAGE_DIR"
    mkdir -p "$PACKAGE_DIR"

    # Binaire backend
    cp backend/target/release/hortus-backend "$PACKAGE_DIR/"

    # Frontend statique (avec patch backendUrl en relatif)
    mkdir -p "$PACKAGE_DIR/static"
    cp frontend/static/elm.js "$PACKAGE_DIR/static/"
    cp frontend/static/style.css "$PACKAGE_DIR/static/"
    # index.html: patcher backendUrl + cache-bust elm.js / style.css (timestamp deploy)
    BUILD_TS=$(date +%s)
    sed -e 's|backendUrl: *"http://localhost:[0-9]*"|backendUrl: ""|g' \
        -e "s|elm\\.js\"|elm.js?v=${BUILD_TS}\"|g" \
        -e "s|style\\.css\"|style.css?v=${BUILD_TS}\"|g" \
        frontend/static/index.html > "$PACKAGE_DIR/static/index.html"

    # Catalogue espece (necessaire au runtime)
    mkdir -p "$PACKAGE_DIR/data"
    cp backend/data/species.json "$PACKAGE_DIR/data/"

    # ── Service systemd avec sandbox complet ──
    cat > "$PACKAGE_DIR/hortus.service" << EOF
[Unit]
Description=Hortus - Assistant jardinier maraicher
After=network.target

[Service]
Type=simple
User=hortus
Group=hortus
WorkingDirectory=/opt/hortus
ExecStart=/opt/hortus/hortus-backend
Restart=always
RestartSec=5

# Environnement
Environment=RUST_LOG=info
Environment=PORT=${APP_PORT}

# ── Securite : sandbox systemd complet ──
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/hortus/data
PrivateTmp=true
PrivateDevices=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectKernelLogs=true
ProtectControlGroups=true
ProtectClock=true
ProtectHostname=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
RestrictNamespaces=true
RestrictRealtime=true
RestrictSUIDSGID=true
LockPersonality=true
MemoryDenyWriteExecute=true
SystemCallArchitectures=native
SystemCallFilter=@system-service
SystemCallFilter=~@privileged @resources
CapabilityBoundingSet=
AmbientCapabilities=

# Limites ressources
LimitNOFILE=65536
LimitNPROC=512

[Install]
WantedBy=multi-user.target
EOF

    # ── Config nginx ──
    cat > "$PACKAGE_DIR/nginx-hortus.conf" << NGINX_EOF
# Rate limiting zones
limit_req_zone \$binary_remote_addr zone=hortus_page:10m rate=20r/s;
limit_req_zone \$binary_remote_addr zone=hortus_api:10m rate=30r/s;
limit_req_zone \$binary_remote_addr zone=hortus_static:10m rate=50r/s;
limit_conn_zone \$binary_remote_addr zone=hortus_conn:10m;

server {
    listen 80;
    listen [::]:80;
    server_name $DOMAIN;

    # ── Security Headers ──
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-Frame-Options "DENY" always;
    add_header X-XSS-Protection "1; mode=block" always;
    add_header Referrer-Policy "strict-origin-when-cross-origin" always;
    add_header Permissions-Policy "camera=(), microphone=(), geolocation=(), payment=()" always;
    add_header Content-Security-Policy "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; img-src 'self' data:; font-src 'self' https://fonts.gstatic.com; connect-src 'self';" always;

    server_tokens off;

    client_max_body_size 1m;
    client_body_timeout 10s;
    client_header_timeout 10s;

    limit_conn hortus_conn 20;

    # Gzip
    gzip on;
    gzip_vary on;
    gzip_proxied any;
    gzip_comp_level 6;
    gzip_types text/plain text/css application/json application/javascript text/xml application/xml text/javascript image/svg+xml;

    # ── Routes API → backend Rust ──
    location ~ ^/(health|species|cities|calendar|forecast|historical-year|action-kinds|parcels|actions|problems)(/|$) {
        limit_req zone=hortus_api burst=40 nodelay;
        limit_req_status 429;

        proxy_pass http://127.0.0.1:${APP_PORT};
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;

        proxy_buffering on;
        proxy_buffer_size 8k;
        proxy_buffers 8 8k;

        proxy_read_timeout 30s;
        proxy_send_timeout 10s;
        proxy_connect_timeout 5s;
    }

    # ── Frontend statique ──
    location / {
        root /opt/hortus/static;
        try_files \$uri \$uri/ /index.html;
        limit_req zone=hortus_static burst=50 nodelay;
        expires 1h;
        add_header Cache-Control "public";
    }

    # Bloquer les fichiers caches
    location ~ /\. {
        deny all;
        return 404;
    }

    # Bloquer les extensions dangereuses
    location ~* \.(php|asp|aspx|jsp|cgi|pl|py|sh|bash|env|git|svn|htaccess|htpasswd|ini|log|sql|bak|swp|tmp)$ {
        deny all;
        return 404;
    }
}
NGINX_EOF

    # ── Script d'installation sur le VPS ──
    cat > "$PACKAGE_DIR/install.sh" << 'INSTALL_EOF'
#!/bin/bash
set -e

echo "=== Installation de Hortus ==="

# ── Creer l'utilisateur systeme ──
if ! id "hortus" &>/dev/null; then
    useradd -r -s /usr/sbin/nologin -d /opt/hortus -M hortus
    echo "Utilisateur hortus cree (nologin)"
fi

# ── Repertoires ──
mkdir -p /opt/hortus/{data,static}

# Stopper le service si actif (sinon binaire "Text file busy")
if systemctl is-active --quiet hortus 2>/dev/null; then
    systemctl stop hortus
    echo "Service hortus stoppe pour mise a jour"
fi

# Sauvegarder la DB existante avant copie
if [ -f /opt/hortus/data/hortus.db ]; then
    cp /opt/hortus/data/hortus.db /opt/hortus/data/hortus.db.bak
    echo "DB existante sauvegardee: /opt/hortus/data/hortus.db.bak"
fi

# ── Copier les fichiers ──
cp hortus-backend /opt/hortus/
chmod 750 /opt/hortus/hortus-backend

cp -r static/* /opt/hortus/static/

# species.json: ecraser (versionne dans le repo)
cp data/species.json /opt/hortus/data/species.json

# ── Permissions strictes ──
chown -R hortus:hortus /opt/hortus
chmod 755 /opt/hortus
chmod 755 /opt/hortus/static
find /opt/hortus/static -type f -exec chmod 644 {} \;
find /opt/hortus/static -type d -exec chmod 755 {} \;
chmod 750 /opt/hortus/data
chmod 644 /opt/hortus/data/species.json

# nginx doit pouvoir lire static/
chmod o+x /opt/hortus
chmod -R o+rX /opt/hortus/static

# ── systemd ──
cp hortus.service /etc/systemd/system/
chmod 644 /etc/systemd/system/hortus.service
systemctl daemon-reload
systemctl enable hortus

# ── nginx ──
cp nginx-hortus.conf /etc/nginx/sites-available/hortus
ln -sf /etc/nginx/sites-available/hortus /etc/nginx/sites-enabled/

if ! grep -q "server_tokens off" /etc/nginx/nginx.conf; then
    sed -i '/http {/a\    server_tokens off;' /etc/nginx/nginx.conf
fi

nginx -t && systemctl reload nginx

# ── Firewall (deja configure par fouduvolant probablement) ──
if command -v ufw >/dev/null 2>&1; then
    ufw allow 80/tcp 2>/dev/null || true
    ufw allow 443/tcp 2>/dev/null || true
fi

# ── Fail2ban ──
if command -v fail2ban-client >/dev/null 2>&1 && [ ! -f /etc/fail2ban/jail.d/hortus.conf ]; then
    cat > /etc/fail2ban/jail.d/hortus.conf << 'F2B_EOF'
[nginx-limit-req]
enabled = true
port = http,https
logpath = /var/log/nginx/error.log
maxretry = 10
findtime = 60
bantime = 600
F2B_EOF
    systemctl restart fail2ban 2>/dev/null || true
fi

# ── Demarrer ──
systemctl restart hortus

echo ""
echo "=== Installation terminee ==="
echo "Service: systemctl status hortus"
echo "Logs:    journalctl -u hortus -f"
echo ""
echo "Prochaines etapes:"
echo "  1. Pointer DNS hortus.eigenplay.com -> IP du VPS"
echo "  2. HTTPS: certbot --nginx -d hortus.eigenplay.com"
echo ""
INSTALL_EOF
    chmod +x "$PACKAGE_DIR/install.sh"

    PACKAGE_SIZE=$(du -sh "$PACKAGE_DIR" | cut -f1)
    log_success "Package cree ($PACKAGE_SIZE)"
}

# ============================================================================
# DEPLOIEMENT
# ============================================================================

deploy_to_vps() {
    log_info "Deploiement sur le VPS ($VPS_USER@$VPS_HOST:$VPS_PORT)..."
    cd "$PROJECT_DIR"

    log_info "Test de connexion SSH..."
    ssh -p "$VPS_PORT" -o ConnectTimeout=10 "$VPS_USER@$VPS_HOST" "echo 'Connexion OK'" \
        || log_error "Impossible de se connecter au VPS"

    ssh -p "$VPS_PORT" "$VPS_USER@$VPS_HOST" "mkdir -p /tmp/hortus_deploy"

    log_info "Transfert des fichiers..."
    scp -P "$VPS_PORT" -r deploy_package/* "$VPS_USER@$VPS_HOST:/tmp/hortus_deploy/"

    log_info "Installation sur le VPS..."
    ssh -p "$VPS_PORT" "$VPS_USER@$VPS_HOST" "cd /tmp/hortus_deploy && bash install.sh"

    ssh -p "$VPS_PORT" "$VPS_USER@$VPS_HOST" "rm -rf /tmp/hortus_deploy"

    log_success "Deploiement termine!"
}

setup_vps_prerequisites() {
    log_info "Installation des prerequis sur le VPS..."

    ssh -p "$VPS_PORT" "$VPS_USER@$VPS_HOST" << 'REMOTE_EOF'
set -e
export DEBIAN_FRONTEND=noninteractive
# Reparer dpkg si interrompu precedemment
dpkg --configure -a 2>/dev/null || true
apt-get update
apt-get install -y nginx ufw fail2ban
apt-get install -y unattended-upgrades
dpkg-reconfigure -plow unattended-upgrades 2>/dev/null || true
mkdir -p /tmp/hortus_deploy
echo "Prerequis installes"
REMOTE_EOF

    log_success "Prerequis VPS OK"
}

# ============================================================================
# COMMANDES
# ============================================================================

show_help() {
    echo "Usage: $0 <commande>"
    echo ""
    echo "Commandes:"
    echo "  build       Compiler backend (Docker) + frontend (Elm)"
    echo "  package     Creer le package de deploiement"
    echo "  deploy      Deployer sur le VPS (build + package + upload)"
    echo "  setup-vps   Installer les prerequis VPS (nginx, ufw, fail2ban)"
    echo "  full        setup-vps + build + package + deploy"
    echo "  status      Statut du service sur le VPS"
    echo "  logs        Logs du service sur le VPS"
    echo "  restart     Redemarrer le service"
    echo ""
    echo "Premier deploiement:"
    echo "  1. $0 full"
    echo "  2. Pointer DNS $DOMAIN -> IP VPS"
    echo "  3. ssh -p $VPS_PORT $VPS_USER@$VPS_HOST 'certbot --nginx -d $DOMAIN'"
    echo ""
}

cmd_build() {
    check_dependencies
    build_backend
    build_frontend
    log_success "Build complet termine!"
}

cmd_package() {
    create_package
}

cmd_deploy() {
    check_config
    check_dependencies
    build_backend
    build_frontend
    create_package
    deploy_to_vps
}

cmd_full() {
    check_config
    check_dependencies
    setup_vps_prerequisites
    build_backend
    build_frontend
    create_package
    deploy_to_vps

    echo ""
    log_success "=== Deploiement complet termine! ==="
    echo ""
    echo "Site: http://$DOMAIN (apres DNS)"
    echo ""
    echo "  $0 status   - Statut du service"
    echo "  $0 logs     - Logs"
    echo "  $0 restart  - Redemarrer"
    echo ""
    echo "HTTPS:"
    echo "  ssh -p $VPS_PORT $VPS_USER@$VPS_HOST"
    echo "  certbot --nginx -d $DOMAIN"
    echo ""
}

cmd_status() {
    check_config
    ssh -p "$VPS_PORT" "$VPS_USER@$VPS_HOST" "systemctl status hortus"
}

cmd_logs() {
    check_config
    ssh -p "$VPS_PORT" "$VPS_USER@$VPS_HOST" "journalctl -u hortus -f"
}

cmd_restart() {
    check_config
    ssh -p "$VPS_PORT" "$VPS_USER@$VPS_HOST" "systemctl restart hortus"
    log_success "Service redemarre"
}

# ============================================================================
# MAIN
# ============================================================================

case "${1:-help}" in
    build)      cmd_build ;;
    package)    cmd_package ;;
    deploy)     cmd_deploy ;;
    setup-vps)  check_config; setup_vps_prerequisites ;;
    full)       cmd_full ;;
    status)     cmd_status ;;
    logs)       cmd_logs ;;
    restart)    cmd_restart ;;
    help|--help|-h) show_help ;;
    *)          log_error "Commande inconnue: $1. Utilisez '$0 help'" ;;
esac
