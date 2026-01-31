# Bark - Build System
#
# Build everything:      make
# Build release:         make release
# Build debug:           make debug
# Install plugins:       make install-plugins
# Clean:                 make clean

PLUGIN_DIR := $(HOME)/.config/bark/plugins
TARGET     := target

WIN_TARGET := x86_64-pc-windows-gnu

.PHONY: all release debug clean install-plugins install-plugins-debug setup setup-deps setup-cross windows help

# Default: build everything in release mode
all: release

# Build everything (bark + all plugins) in release mode
release:
	cargo build --release --workspace

# Build everything in debug mode
debug:
	cargo build --workspace

# Install runtime dependencies (script command, libssh2, etc.)
setup-deps:
	@echo "Installing runtime dependencies..."
	@if [ "$$(uname)" = "Darwin" ]; then \
		echo "macOS detected — runtime deps are pre-installed."; \
	elif command -v apt-get >/dev/null 2>&1; then \
		echo "Detected Debian/Ubuntu..."; \
		sudo apt-get update && sudo apt-get install -y bsdutils libssh2-1-dev; \
	elif command -v dnf >/dev/null 2>&1; then \
		echo "Detected Fedora/RHEL..."; \
		sudo dnf install -y util-linux-script libssh2-devel gcc-c++; \
	elif command -v pacman >/dev/null 2>&1; then \
		echo "Detected Arch..."; \
		sudo pacman -S --needed util-linux libssh2; \
	else \
		echo "Could not detect package manager."; \
		echo "Please install manually: script (from util-linux), libssh2-dev"; \
	fi
	@echo "Runtime dependencies installed."

# Install prerequisites for cross-compilation to Windows
setup-cross:
	@echo "Installing cross-compilation prerequisites..."
	@# Rust Windows target
	rustup target add $(WIN_TARGET)
	@# System cross-compiler
	@if command -v apt-get >/dev/null 2>&1; then \
		echo "Detected Debian/Ubuntu — installing mingw-w64..."; \
		sudo apt-get update && sudo apt-get install -y gcc-mingw-w64-x86-64; \
	elif command -v dnf >/dev/null 2>&1; then \
		echo "Detected Fedora/RHEL — installing mingw64-gcc..."; \
		sudo dnf install -y mingw64-gcc mingw64-gcc-c++; \
	elif command -v pacman >/dev/null 2>&1; then \
		echo "Detected Arch — installing mingw-w64-gcc..."; \
		sudo pacman -S --needed mingw-w64-gcc; \
	elif command -v brew >/dev/null 2>&1; then \
		echo "Detected macOS — installing mingw-w64..."; \
		brew install mingw-w64; \
	else \
		echo "Could not detect package manager. Install mingw-w64 manually."; \
		exit 1; \
	fi
	@echo "Cross-compilation setup complete. Run 'make windows' to cross-compile."

# Install all prerequisites (runtime deps + cross-compilation)
setup: setup-deps setup-cross

# Cross-compile for Windows (x86_64)
# vendor/mingw-case-fix/ has wrapper headers for PowrProf.h / Wbemidl.h whose
# lowercase names on mingw don't match the mixed-case #includes in unrar.
windows:
	@echo "Cross-compiling for Windows ($(WIN_TARGET))..."
	@rustup target list --installed | grep -q $(WIN_TARGET) || \
		(echo "Installing target $(WIN_TARGET)..." && rustup target add $(WIN_TARGET))
	CXXFLAGS_x86_64_pc_windows_gnu="-I$(CURDIR)/plugins/archive-plugin/vendor/mingw-case-fix" \
		cargo build --release --target $(WIN_TARGET) --workspace
	@echo ""
	@echo "Windows binaries:"
	@echo "  $(TARGET)/$(WIN_TARGET)/release/ba.exe"
	@ls $(TARGET)/$(WIN_TARGET)/release/bark-*.exe 2>/dev/null | sed 's/^/  /' || true

# Clean all build artifacts
clean:
	cargo clean

# Install plugins to ~/.config/bark/plugins/
install-plugins: release
	@echo "Installing plugins to $(PLUGIN_DIR)..."
	@mkdir -p $(PLUGIN_DIR)
	cp $(TARGET)/release/bark-ftp $(PLUGIN_DIR)/
	cp $(TARGET)/release/bark-webdav $(PLUGIN_DIR)/
	cp $(TARGET)/release/bark-archive $(PLUGIN_DIR)/
	cp $(TARGET)/release/bark-elf-viewer $(PLUGIN_DIR)/
	cp $(TARGET)/release/bark-pe-viewer $(PLUGIN_DIR)/
	cp $(TARGET)/release/bark-image-viewer $(PLUGIN_DIR)/
	cp $(TARGET)/release/bark-pdf-viewer $(PLUGIN_DIR)/
	cp plugins/scripts/*.py $(PLUGIN_DIR)/ 2>/dev/null || true
	cp plugins/scripts/*.sh $(PLUGIN_DIR)/ 2>/dev/null || true
	chmod +x $(PLUGIN_DIR)/* 2>/dev/null || true
	@echo "Plugins installed to $(PLUGIN_DIR)"

# Install debug plugin builds
install-plugins-debug: debug
	@mkdir -p $(PLUGIN_DIR)
	cp $(TARGET)/debug/bark-ftp $(PLUGIN_DIR)/
	cp $(TARGET)/debug/bark-webdav $(PLUGIN_DIR)/
	cp $(TARGET)/debug/bark-archive $(PLUGIN_DIR)/
	cp $(TARGET)/debug/bark-elf-viewer $(PLUGIN_DIR)/
	cp $(TARGET)/debug/bark-pe-viewer $(PLUGIN_DIR)/
	cp $(TARGET)/debug/bark-image-viewer $(PLUGIN_DIR)/
	cp $(TARGET)/debug/bark-pdf-viewer $(PLUGIN_DIR)/
	cp plugins/scripts/*.py $(PLUGIN_DIR)/ 2>/dev/null || true
	chmod +x $(PLUGIN_DIR)/* 2>/dev/null || true
	@echo "Debug plugins installed to $(PLUGIN_DIR)"

# Help
help:
	@echo "Bark Build System"
	@echo ""
	@echo "Targets:"
	@echo "  all                 Build everything (release mode)"
	@echo "  release             Build everything (release mode)"
	@echo "  debug               Build everything (debug mode)"
	@echo "  install-plugins     Build and install plugins to $(PLUGIN_DIR)"
	@echo "  install-plugins-debug  Install debug plugin builds"
	@echo "  windows             Cross-compile for Windows (x86_64)"
	@echo "  setup               Install all prerequisites (deps + cross-compile)"
	@echo "  setup-deps          Install runtime dependencies (script, libssh2)"
	@echo "  setup-cross         Install cross-compile toolchain (mingw-w64)"
	@echo "  clean               Clean all build artifacts"
	@echo ""
	@echo "Output:"
	@echo "  $(TARGET)/release/ba              Bark binary"
	@echo "  $(TARGET)/release/bark-ftp        FTP plugin"
	@echo "  $(TARGET)/release/bark-webdav    WebDAV plugin"
	@echo "  $(TARGET)/release/bark-archive   Archive plugin"
	@echo "  $(TARGET)/release/bark-elf-viewer ELF viewer plugin"
	@echo "  $(TARGET)/release/bark-pe-viewer  PE viewer plugin"
	@echo "  $(TARGET)/release/bark-image-viewer Image viewer plugin"
	@echo "  $(TARGET)/release/bark-pdf-viewer  PDF viewer plugin"
