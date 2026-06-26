"""
Tunnel Manager for Riffusion Kaggle Song Studio

Manages cloudflared tunnel creation for exposing the FastAPI server
to the public internet from within a Kaggle notebook.
"""

import os
import subprocess
import time
import re
from pathlib import Path
from typing import Optional, Tuple
import logging
import threading

logger = logging.getLogger(__name__)


class TunnelManager:
    """
    Manages cloudflared tunnel for exposing local server.
    
    Creates a secure tunnel from the Kaggle notebook to cloudflare,
    providing a public URL for the API server.
    """
    
    def __init__(self):
        """Initialize tunnel manager."""
        self.tunnel_process: Optional[subprocess.Popen] = None
        self.tunnel_url: Optional[str] = None
        self.is_running = False
        self._log_thread: Optional[threading.Thread] = None
    
    def install_cloudflared(self) -> bool:
        """
        Install cloudflared in the Kaggle environment.
        
        Returns:
            True if installation successful
        """
        try:
            # Check if already installed
            result = subprocess.run(
                ["which", "cloudflared"],
                capture_output=True,
                text=True
            )
            
            if result.returncode == 0:
                logger.info("cloudflared already installed")
                return True
            
            # Download and install cloudflared
            logger.info("Installing cloudflared...")
            
            # Download latest version
            download_cmd = (
                "curl -L --output /tmp/cloudflared.deb "
                "https://github.com/cloudflare/cloudflared/releases/latest/download/"
                "cloudflared-linux-amd64.deb"
            )
            
            result = subprocess.run(
                download_cmd,
                shell=True,
                capture_output=True,
                text=True
            )
            
            if result.returncode != 0:
                logger.error(f"Download failed: {result.stderr}")
                return False
            
            # Install deb package
            install_cmd = "dpkg -i /tmp/cloudflared.deb"
            result = subprocess.run(
                install_cmd,
                shell=True,
                capture_output=True,
                text=True
            )
            
            if result.returncode != 0:
                logger.error(f"Installation failed: {result.stderr}")
                return False
            
            logger.info("cloudflared installed successfully")
            return True
            
        except Exception as e:
            logger.error(f"Installation error: {e}")
            return False
    
    def start_tunnel(self, port: int = 8000) -> Tuple[bool, Optional[str]]:
        """
        Start a cloudflared tunnel to expose the local server.
        
        Args:
            port: Local port to expose
        
        Returns:
            Tuple of (success, public_url)
        """
        if self.is_running:
            logger.warning("Tunnel already running")
            return True, self.tunnel_url
        
        # Ensure cloudflared is installed
        if not self.install_cloudflared():
            return False, None
        
        try:
            # Start cloudflared tunnel
            # Using quick tunnel mode (no account required)
            cmd = [
                "cloudflared",
                "tunnel",
                "--url",
                f"http://localhost:{port}"
            ]
            
            logger.info(f"Starting tunnel: {' '.join(cmd)}")
            
            self.tunnel_process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1
            )
            
            # Start log reader thread
            self._log_thread = threading.Thread(
                target=self._read_tunnel_logs,
                daemon=True
            )
            self._log_thread.start()
            
            # Wait for tunnel URL to appear
            max_wait = 30  # seconds
            start_time = time.time()
            
            while time.time() - start_time < max_wait:
                if self.tunnel_url:
                    self.is_running = True
                    logger.info(f"Tunnel started: {self.tunnel_url}")
                    return True, self.tunnel_url
                
                # Check if process died
                if self.tunnel_process.poll() is not None:
                    logger.error("Tunnel process exited unexpectedly")
                    return False, None
                
                time.sleep(1)
            
            logger.error("Timeout waiting for tunnel URL")
            return False, None
            
        except Exception as e:
            logger.error(f"Tunnel start error: {e}")
            return False, None
    
    def _read_tunnel_logs(self) -> None:
        """Read and parse tunnel logs to extract public URL."""
        if not self.tunnel_process or not self.tunnel_process.stdout:
            return
        
        # Pattern to match tunnel URL
        url_pattern = re.compile(
            r'https://[a-zA-Z0-9-]+\.trycloudflare\.com'
        )
        
        for line in iter(self.tunnel_process.stdout.readline, ''):
            if line:
                logger.debug(f"cloudflared: {line.strip()}")
                
                # Look for URL in logs
                match = url_pattern.search(line)
                if match and not self.tunnel_url:
                    self.tunnel_url = match.group(0)
                    logger.info(f"Found tunnel URL: {self.tunnel_url}")
    
    def stop_tunnel(self) -> None:
        """Stop the tunnel."""
        if not self.is_running:
            return
        
        logger.info("Stopping tunnel...")
        
        if self.tunnel_process:
            try:
                self.tunnel_process.terminate()
                self.tunnel_process.wait(timeout=5)
            except Exception as e:
                logger.error(f"Error stopping tunnel: {e}")
                if self.tunnel_process:
                    self.tunnel_process.kill()
        
        self.tunnel_process = None
        self.tunnel_url = None
        self.is_running = False
    
    def get_tunnel_url(self) -> Optional[str]:
        """Get the current tunnel URL."""
        return self.tunnel_url
    
    def is_tunnel_running(self) -> bool:
        """Check if tunnel is currently running."""
        return self.is_running and self.tunnel_url is not None
    
    def get_connection_info(self) -> dict:
        """Get complete connection information for clients."""
        return {
            "tunnel_url": self.tunnel_url,
            "is_running": self.is_running,
            "local_port": 8000 if self.is_running else None
        }


def create_quick_tunnel(port: int = 8000) -> Optional[str]:
    """
    Convenience function to create a quick tunnel.
    
    Args:
        port: Local port to expose
    
    Returns:
        Public tunnel URL or None if failed
    """
    manager = TunnelManager()
    success, url = manager.start_tunnel(port)
    
    if success:
        return url
    return None


if __name__ == "__main__":
    # Test tunnel creation
    print("Testing cloudflared tunnel...")
    
    manager = TunnelManager()
    success, url = manager.start_tunnel(8000)
    
    if success and url:
        print(f"\n✅ Tunnel created successfully!")
        print(f"Public URL: {url}")
        print(f"\nKeep this script running to maintain the tunnel.")
        print("Press Ctrl+C to stop.\n")
        
        try:
            while True:
                time.sleep(1)
        except KeyboardInterrupt:
            print("\nStopping tunnel...")
            manager.stop_tunnel()
            print("Tunnel stopped.")
    else:
        print("❌ Failed to create tunnel")
