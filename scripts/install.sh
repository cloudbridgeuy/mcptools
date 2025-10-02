#!/usr/bin/env bash
set -euo pipefail

# MCPTOOLS DevOps CLI Installation Script
# This script downloads and installs the latest release of mcptools

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
REPO="cloudbridgeuy/mcptools"
BINARY_NAME="mcptools"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Functions
log_info() {
	echo -e "${BLUE}INFO:${NC} $1"
}

log_success() {
	echo -e "${GREEN}SUCCESS:${NC} $1"
}

log_warning() {
	echo -e "${YELLOW}WARNING:${NC} $1"
}

log_error() {
	echo -e "${RED}ERROR:${NC} $1"
}

# Detect platform
detect_platform() {
	local os arch
	os="$(uname -s)"
	arch="$(uname -m)"

	case "$os" in
	Linux*)
		case "$arch" in
		x86_64) echo "Linux-x86_64" ;;
		aarch64 | arm64) echo "Linux-aarch64" ;;
		*)
			log_error "Unsupported architecture: $arch"
			exit 1
			;;
		esac
		;;
	Darwin*)
		case "$arch" in
		x86_64) echo "Darwin-x86_64" ;;
		arm64) echo "Darwin-arm64" ;;
		*)
			log_error "Unsupported architecture: $arch"
			exit 1
			;;
		esac
		;;
	CYGWIN* | MINGW* | MSYS*)
		case "$arch" in
		x86_64) echo "Windows-x86_64" ;;
		*)
			log_error "Unsupported architecture: $arch"
			exit 1
			;;
		esac
		;;
	*)
		log_error "Unsupported operating system: $os"
		exit 1
		;;
	esac
}

# Get latest release version
get_latest_version() {
	local api_url="https://api.github.com/repos/$REPO/releases/latest"

	if command -v curl >/dev/null 2>&1; then
		curl -s "$api_url" | grep '"tag_name":' | sed -E 's/.*"tag_name": "([^"]+)".*/\1/'
	elif command -v wget >/dev/null 2>&1; then
		wget -qO- "$api_url" | grep '"tag_name":' | sed -E 's/.*"tag_name": "([^"]+)".*/\1/'
	else
		log_error "Neither curl nor wget is available. Please install one of them."
		exit 1
	fi
}

# Download binary
download_binary() {
	local version="$1"
	local platform="$2"
	local temp_file="/tmp/${BINARY_NAME}-${platform}"
	local download_url="https://github.com/$REPO/releases/download/$version/${BINARY_NAME}-$platform"

	# Add .exe extension for Windows
	if [[ "$platform" == *"Windows"* ]]; then
		download_url="${download_url}.exe"
		temp_file="${temp_file}.exe"
	fi

	log_info "Downloading $BINARY_NAME $version for $platform..."

	if command -v curl >/dev/null 2>&1; then
		if ! curl -L "$download_url" -o "$temp_file"; then
			log_error "Failed to download binary from $download_url"
			exit 1
		fi
	elif command -v wget >/dev/null 2>&1; then
		if ! wget -O "$temp_file" "$download_url"; then
			log_error "Failed to download binary from $download_url"
			exit 1
		fi
	else
		log_error "Neither curl nor wget is available. Please install one of them."
		exit 1
	fi

	echo "$temp_file"
}

# Install binary
install_binary() {
	local temp_file="$1"
	local install_path="$INSTALL_DIR/$BINARY_NAME"

	# Make executable
	chmod +x "$temp_file"

	# Create install directory if it doesn't exist
	if [[ ! -d "$INSTALL_DIR" ]]; then
		log_info "Creating install directory: $INSTALL_DIR"
		if ! mkdir -p "$INSTALL_DIR"; then
			log_error "Failed to create install directory. You may need to run with sudo."
			exit 1
		fi
	fi

	# Install binary
	log_info "Installing $BINARY_NAME to $install_path..."
	if ! mv "$temp_file" "$install_path"; then
		log_error "Failed to install binary. You may need to run with sudo."
		log_info "Try running: sudo $0"
		exit 1
	fi

	log_success "$BINARY_NAME installed successfully to $install_path"
}

# Verify installation
verify_installation() {
	if command -v "$BINARY_NAME" >/dev/null 2>&1; then
		local version
		version="$($BINARY_NAME --version 2>/dev/null || echo 'unknown')"
		log_success "Installation verified! $BINARY_NAME version: $version"

		# Show help if available
		log_info "Run '$BINARY_NAME --help' to get started."
	else
		log_warning "Binary installed but not found in PATH."
		log_info "Make sure $INSTALL_DIR is in your PATH environment variable."
		log_info "You can add it by running:"
		log_info "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.bashrc"
		log_info "  source ~/.bashrc"
	fi
}

# Main function
main() {
	local version platform temp_file

	log_info "MCPTOOLS DevOps CLI Installation Script"
	echo

	# Detect platform
	platform=$(detect_platform)
	log_info "Detected platform: $platform"

	# Get latest version
	log_info "Fetching latest release information..."
	version=$(get_latest_version)
	if [[ -z "$version" ]]; then
		log_error "Failed to fetch latest version information"
		exit 1
	fi
	log_info "Latest version: $version"

	# Check if already installed
	if command -v "$BINARY_NAME" >/dev/null 2>&1; then
		local current_version
		current_version="$($BINARY_NAME --version 2>/dev/null | awk '{print $2}' || echo 'unknown')"
		log_info "Current installed version: $current_version"

		if [[ "$current_version" == "${version#v}" ]]; then
			log_info "Latest version already installed!"
			exit 0
		fi
	fi

	# Download binary
	temp_file=$(download_binary "$version" "$platform")

	# Install binary
	install_binary "$temp_file"

	# Verify installation
	verify_installation

	log_success "Installation completed successfully!"
}

# Show usage
usage() {
	echo "MCPTOOLS DevOps CLI Installation Script"
	echo
	echo "Usage: $0 [OPTIONS]"
	echo
	echo "Options:"
	echo "  -h, --help          Show this help message"
	echo "  -d, --install-dir   Installation directory (default: /usr/local/bin)"
	echo
	echo "Environment Variables:"
	echo "  INSTALL_DIR         Installation directory (overrides default)"
	echo
	echo "Examples:"
	echo "  $0                          # Install to /usr/local/bin"
	echo "  $0 -d ~/.local/bin          # Install to ~/.local/bin"
	echo "  INSTALL_DIR=~/bin $0        # Install to ~/bin"
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
	case $1 in
	-h | --help)
		usage
		exit 0
		;;
	-d | --install-dir)
		INSTALL_DIR="$2"
		shift 2
		;;
	*)
		log_error "Unknown option: $1"
		usage
		exit 1
		;;
	esac
done

# Run main function
main
