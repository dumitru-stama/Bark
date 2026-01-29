#!/usr/bin/env python3
"""
System Status plugin for Bark

This plugin shows system memory and CPU info in the status bar.

Usage:
  - Make executable: chmod +x system_status.py
  - Place in plugins/scripts/
  - Bark will load it automatically
"""

import sys
import json
import os

def get_plugin_info():
    """Return plugin metadata."""
    return {
        "name": "System Status",
        "version": "1.0.0",
        "type": "status",
        "description": "Shows system memory and CPU load in status bar",
        "icon": "📊"
    }

def get_memory_info():
    """Get memory usage (cross-platform)."""
    try:
        # Try Linux /proc/meminfo first
        if os.path.exists('/proc/meminfo'):
            with open('/proc/meminfo', 'r') as f:
                lines = f.readlines()

            mem = {}
            for line in lines:
                parts = line.split()
                if len(parts) >= 2:
                    key = parts[0].rstrip(':')
                    value = int(parts[1])
                    mem[key] = value

            total = mem.get('MemTotal', 0) // 1024  # MB
            available = mem.get('MemAvailable', mem.get('MemFree', 0)) // 1024  # MB
            used = total - available

            return f"Mem: {used}/{total}MB"

        # macOS alternative
        elif sys.platform == 'darwin':
            import subprocess
            result = subprocess.run(['vm_stat'], capture_output=True, text=True)
            # Simplified - just show we're on macOS
            return "Mem: macOS"

        # Windows alternative
        elif sys.platform == 'win32':
            # Could use ctypes to call GlobalMemoryStatusEx, but keep it simple
            return "Mem: Windows"

        else:
            return "Mem: N/A"

    except Exception as e:
        return f"Mem: Error"

def get_load_average():
    """Get system load average (Unix only)."""
    try:
        if hasattr(os, 'getloadavg'):
            load1, load5, load15 = os.getloadavg()
            return f"Load: {load1:.1f}"
        else:
            return ""
    except:
        return ""

def render_status(context):
    """Render the status bar section."""
    mem = get_memory_info()
    load = get_load_average()

    if load:
        return {"text": f"{mem} | {load}"}
    else:
        return {"text": mem}

def main():
    # Check if we're being queried for info
    if len(sys.argv) > 1 and sys.argv[1] == '--plugin-info':
        print(json.dumps(get_plugin_info()))
        return

    # Read command from stdin
    try:
        line = sys.stdin.readline().strip()
        if not line:
            return

        request = json.loads(line)
        command = request.get('command', '')

        if command == 'status_render':
            result = render_status(request)
            print(json.dumps(result))
        else:
            print(json.dumps({"error": f"Unknown command: {command}"}))

    except json.JSONDecodeError as e:
        print(json.dumps({"error": f"Invalid JSON: {e}"}))
    except Exception as e:
        print(json.dumps({"error": str(e)}))

if __name__ == '__main__':
    main()
