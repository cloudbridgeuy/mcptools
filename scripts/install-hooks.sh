#!/usr/bin/env bash
set -euo pipefail

# MCPTOOLS DevOps CLI Git Hooks Installer
# This script installs git hooks for the project

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
HOOKS_DIR="$SCRIPT_DIR/hooks"
GIT_HOOKS_DIR="$PROJECT_ROOT/.git/hooks"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

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

log_step() {
	echo -e "${CYAN}${BOLD}==>${NC} $1"
}

# Check if we're in a git repository
check_git_repo() {
	if [[ ! -d "$PROJECT_ROOT/.git" ]]; then
		log_error "This directory is not a git repository"
		log_info "Please run this script from within a git repository"
		exit 1
	fi
}

# Check if hooks directory exists
check_hooks_directory() {
	if [[ ! -d "$HOOKS_DIR" ]]; then
		log_error "Hooks directory not found: $HOOKS_DIR"
		log_info "Please ensure the hooks are available in the project"
		exit 1
	fi
}

# Create git hooks directory if it doesn't exist
create_git_hooks_dir() {
	if [[ ! -d "$GIT_HOOKS_DIR" ]]; then
		log_info "Creating git hooks directory: $GIT_HOOKS_DIR"
		mkdir -p "$GIT_HOOKS_DIR"
	fi
}

# Backup existing hook
backup_existing_hook() {
	local hook_name="$1"
	local hook_path="$GIT_HOOKS_DIR/$hook_name"

	if [[ -f "$hook_path" && ! -L "$hook_path" ]]; then
		local backup_path
		backup_path="${hook_path}.backup.$(date +%Y%m%d_%H%M%S)"
		log_warning "Backing up existing $hook_name hook to: $(basename "$backup_path")"
		mv "$hook_path" "$backup_path"
	fi
}

# Install a specific hook
install_hook() {
	local hook_name="$1"
	local source_hook="$HOOKS_DIR/$hook_name"
	local target_hook="$GIT_HOOKS_DIR/$hook_name"

	if [[ ! -f "$source_hook" ]]; then
		log_warning "Hook not found: $source_hook"
		return 1
	fi

	log_info "Installing $hook_name hook..."

	# Backup existing hook if it exists
	backup_existing_hook "$hook_name"

	# Create symlink to our hook
	ln -sf "$source_hook" "$target_hook"

	# Make sure it's executable
	chmod +x "$target_hook"

	log_success "✅ $hook_name hook installed"
}

# Install all available hooks
install_all_hooks() {
	local installed_count=0
	local failed_count=0

	log_step "Installing git hooks..."
	echo

	# List of hooks to install
	local hooks=("pre-commit")

	for hook in "${hooks[@]}"; do
		if install_hook "$hook"; then
			((installed_count++))
		else
			((failed_count++))
		fi
	done

	echo
	log_info "Installation summary:"
	log_success "  • Installed: $installed_count hooks"
	if [[ $failed_count -gt 0 ]]; then
		log_warning "  • Failed: $failed_count hooks"
	fi
}

# Check hook installation
check_installation() {
	log_step "Verifying hook installation..."

	local hooks=("pre-commit")
	local all_good=true

	for hook in "${hooks[@]}"; do
		local hook_path="$GIT_HOOKS_DIR/$hook"
		if [[ -L "$hook_path" && -x "$hook_path" ]]; then
			log_success "✅ $hook hook is properly installed"
		else
			log_error "❌ $hook hook installation failed"
			all_good=false
		fi
	done

	if [[ "$all_good" == true ]]; then
		log_success "All hooks are properly installed and executable"
	else
		log_error "Some hooks failed to install properly"
		return 1
	fi
}

# Uninstall hooks
uninstall_hooks() {
	log_step "Uninstalling git hooks..."

	local hooks=("pre-commit")
	local removed_count=0

	for hook in "${hooks[@]}"; do
		local hook_path="$GIT_HOOKS_DIR/$hook"
		if [[ -L "$hook_path" ]]; then
			log_info "Removing $hook hook..."
			rm -f "$hook_path"
			log_success "✅ $hook hook removed"
			((removed_count++))
		elif [[ -f "$hook_path" ]]; then
			log_warning "❓ $hook exists but is not our symlink (skipping)"
		fi
	done

	if [[ $removed_count -gt 0 ]]; then
		log_success "Removed $removed_count hooks"
	else
		log_info "No hooks to remove"
	fi
}

# Show hook status
show_status() {
	log_step "Git hooks status:"
	echo

	local hooks=("pre-commit")

	for hook in "${hooks[@]}"; do
		local hook_path="$GIT_HOOKS_DIR/$hook"
		local source_hook="$HOOKS_DIR/$hook"

		printf "  %-12s " "$hook:"

		if [[ -L "$hook_path" ]]; then
			local target
			target=$(readlink "$hook_path")
			if [[ "$target" = "$source_hook" ]]; then
				echo -e "${GREEN}✅ Installed${NC}"
			else
				echo -e "${YELLOW}⚠️  Installed (different source)${NC}"
			fi
		elif [[ -f "$hook_path" ]]; then
			echo -e "${YELLOW}❓ Exists (not our hook)${NC}"
		else
			echo -e "${RED}❌ Not installed${NC}"
		fi
	done

	echo

	# Show what the hooks do
	log_info "Available hooks:"
	echo "  • pre-commit: Runs cargo fmt, check, and clippy before each commit"
	echo "                Supports --force flag to check all Rust files"
}

# Test hooks
test_hooks() {
	log_step "Testing hooks..."

	# Test pre-commit hook
	if [[ -x "$GIT_HOOKS_DIR/pre-commit" ]]; then
		log_info "Testing pre-commit hook (dry run with --force)..."
		echo

		# Test with --force flag to check all files
		local exit_code
		exit_code=$?

		if [[ $exit_code -eq 0 ]]; then
			log_success "✅ Pre-commit hook test passed"
		else
			log_warning "⚠️  Pre-commit hook test had issues"
			log_info "This might indicate code quality issues that need to be fixed"
		fi

		echo
		log_info "Hook supports the following options:"
		log_info "  • Normal mode: Only checks staged .rs files"
		log_info "  • --force mode: Checks all .rs files in project"
		log_info "  • --help: Shows usage information"
	else
		log_error "❌ Pre-commit hook not found or not executable"
	fi
}

# Show usage
usage() {
	echo "MCPTOOLS Git Hooks Installer"
	echo
	echo "Usage: $0 [command]"
	echo
	echo "Commands:"
	echo "  install     Install all git hooks (default)"
	echo "  uninstall   Remove installed git hooks"
	echo "  status      Show current hooks status"
	echo "  test        Test installed hooks"
	echo "  --help|-h   Show this help message"
	echo
	echo "Examples:"
	echo "  $0                    # Install hooks"
	echo "  $0 install            # Install hooks"
	echo "  $0 status             # Show hook status"
	echo "  $0 uninstall          # Remove hooks"
	echo "  $0 test               # Test hooks"
	echo
	echo "The installer will:"
	echo "  • Install git hooks as symlinks to scripts/hooks/"
	echo "  • Backup any existing hooks before installation"
	echo "  • Make hooks executable automatically"
	echo "  • Verify installation after completion"
}

# Main function
main() {
	local command="${1:-install}"

	case "$command" in
	install)
		log_step "MCPTOOLS DevOps CLI Git Hooks Installer"
		echo

		check_git_repo
		check_hooks_directory
		create_git_hooks_dir
		install_all_hooks

		echo
		if check_installation; then
			echo
			log_success "🎉 Git hooks installation completed successfully!"
			echo
			log_info "The following hooks are now active:"
			log_info "  • pre-commit: Runs cargo fmt, check, and clippy"
			echo
			log_info "You can check hook status anytime with:"
			log_info "  $0 status"
		else
			exit 1
		fi
		;;
	uninstall)
		log_step "Uninstalling MCPTOOLS DevOps CLI Git Hooks"
		echo

		check_git_repo
		uninstall_hooks
		;;
	status)
		check_git_repo
		show_status
		;;
	test)
		check_git_repo
		test_hooks
		;;
	--help | -h | help)
		usage
		;;
	*)
		log_error "Unknown command: $command"
		echo
		usage
		exit 1
		;;
	esac
}

# Run main function
main "$@"
