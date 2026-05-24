#!/bin/sh
# Installer helper: install and enable midjourney-proxy autostart for the current user
# Usage: install_midjourney_service.sh /path/to/app/resource/dir
set -e
RES_DIR="$1"
if [ -z "$RES_DIR" ]; then
  echo "Usage: $0 /path/to/app/resource/dir"
  exit 1
fi
PROXY_DIR="$RES_DIR/midjourney-proxy"
RUN_SH="$PROXY_DIR/run_app.sh"
if [ ! -f "$RUN_SH" ]; then
  RUN_SH="$PROXY_DIR/scripts/run_app.sh"
fi
if [ ! -f "$RUN_SH" ]; then
  PROXY_DIR="$RES_DIR/scripts"
  RUN_SH="$PROXY_DIR/run_app.sh"
fi
if [ ! -f "$RUN_SH" ]; then
  echo "run_app.sh not found in $PROXY_DIR or $PROXY_DIR/scripts"
  exit 1
fi
OS=$(uname)
if [ "$OS" = "Linux" ]; then
  # Install as a user systemd service
  SERVICE_NAME="midjourney-proxy.service"
  SERVICE_PATH="$HOME/.config/systemd/user/$SERVICE_NAME"
  mkdir -p "$(dirname "$SERVICE_PATH")"
  cat > "$SERVICE_PATH" <<EOF
[Unit]
Description=Midjourney Proxy (user service)
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/env sh "$RUN_SH"
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF
  systemctl --user daemon-reload
  systemctl --user enable --now "$SERVICE_NAME"
  echo "Installed and started $SERVICE_NAME (user service)"
  exit 0
elif [ "$OS" = "Darwin" ]; then
  # Install as LaunchAgent
  PLIST_NAME="com.ai.midjourney-proxy.plist"
  PLIST_PATH="$HOME/Library/LaunchAgents/$PLIST_NAME"
  mkdir -p "$(dirname "$PLIST_PATH")"
  cat > "$PLIST_PATH" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>com.ai.midjourney-proxy</string>
    <key>ProgramArguments</key>
    <array>
      <string>/bin/sh</string>
      <string>$RUN_SH</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
  </dict>
</plist>
EOF
  launchctl unload "$PLIST_PATH" 2>/dev/null || true
  launchctl load -w "$PLIST_PATH"
  echo "Installed LaunchAgent at $PLIST_PATH"
  exit 0
else
  # Windows: output a PowerShell script to register a scheduled task
  echo "Windows installation: run the supplied PowerShell script to create a scheduled task that runs on login. See src-tauri/packaging/install_midjourney_service.ps1"
  exit 0
fi
