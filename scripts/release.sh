#!/usr/bin/env bash
set -euo pipefail

# MCPTOOLS DevOps CLI Release Script
# This script helps create and push release tags with failure recovery

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Global variables
GITHUB_REPO="cloudbridgeuy/mcptools"
WORKFLOW_FILE="release.yml"
CI_WORKFLOW_FILE="ci.yml"
WORKFLOW_CHECK_INTERVAL=30
WORKFLOW_TIMEOUT=1800 # 30 minutes
CI_CHECK_TIMEOUT=1800 # 30 minutes
AUTO_UPGRADE=false

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

# Global variables for cleanup
CURRENT_VERSION=""
CURRENT_TAG=""

# Cleanup on exit/interrupt
# shellcheck disable=SC2317
cleanup_on_exit() {
	local exit_code=$?

	if [[ $exit_code -ne 0 && -n "$CURRENT_VERSION" && -n "$CURRENT_TAG" ]]; then
		echo
		log_warning "Script interrupted or failed. Starting cleanup..."
		cleanup_after_failure "$CURRENT_VERSION" 2>/dev/null || true
	fi

	exit "$exit_code"
}

# Set up signal handlers
trap cleanup_on_exit EXIT INT TERM

# Check if we're on the main branch
check_main_branch() {
	log_info "Checking current git branch..."
	local current_branch
	current_branch=$(git branch --show-current)

	if [[ "$current_branch" != "main" ]]; then
		log_error "You must be on the main branch to create a release. Current branch: $current_branch"
		exit 1
	fi
	log_info "On main branch ✓"
}

# Check if working directory is clean
check_clean_working_dir() {
	log_info "Checking working directory status..."
	if [[ -n $(git status --porcelain) ]]; then
		log_error "Working directory is not clean. Please commit or stash your changes."
		git status --short
		exit 1
	fi
	log_info "Working directory is clean ✓"
}

# Get current version from Cargo.toml
get_current_version() {
	grep '^version = ' "$PROJECT_ROOT/Cargo.toml" | sed 's/version = "\(.*\)"/\1/'
	# Check if there's a version in the root Cargo.toml (single crate)
	if grep -q '^version = ' "$PROJECT_ROOT/Cargo.toml" 2>/dev/null; then
		grep '^version = ' "$PROJECT_ROOT/Cargo.toml" | sed 's/version = "\(.*\)"/\1/'
	# Otherwise check in the main crate (workspace)
	elif [[ -f "$PROJECT_ROOT/crates/mcptools/Cargo.toml" ]]; then
		grep '^version = ' "$PROJECT_ROOT/crates/mcptools/Cargo.toml" | sed 's/version = "\(.*\)"/\1/'
	else
		echo "0.0.0" # fallback
	fi
}

# Validate version format (semver)
validate_version() {
	local version="$1"
	log_info "Validating version format: $version"
	if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$ ]]; then
		log_error "Invalid version format: $version. Please use semantic versioning (e.g., 1.0.0, 1.0.0-beta.1)"
		exit 1
	fi
	log_info "Version format is valid ✓"
}

# Check if GitHub CLI is available
check_gh_cli() {
	log_info "Checking GitHub CLI availability..."
	if ! command -v gh >/dev/null 2>&1; then
		log_error "GitHub CLI (gh) is required but not installed."
		log_info "Install it from: https://cli.github.com/"
		exit 1
	fi
	log_info "GitHub CLI found"

	# Check if authenticated
	log_info "Checking GitHub CLI authentication..."
	if ! timeout 10 gh auth status >/dev/null 2>&1; then
		log_error "GitHub CLI is not authenticated or authentication check timed out."
		log_info "Run: gh auth login"
		exit 1
	fi
	log_info "GitHub CLI authenticated successfully"
}

# Check CI workflow status for current commit
check_ci_status() {
	local commit_sha
	commit_sha=$(git rev-parse HEAD)

	log_step "Checking CI workflow status for commit ${commit_sha:0:7}..."

	# Get the CI workflow runs for the current commit
	local workflow_runs
	workflow_runs=$(gh run list \
		--repo="$GITHUB_REPO" \
		--workflow="$CI_WORKFLOW_FILE" \
		--branch=main \
		--limit=10 \
		--json=status,conclusion,headSha,databaseId \
		--jq ".[] | select(.headSha == \"$commit_sha\")")

	if [[ -z "$workflow_runs" ]]; then
		log_warning "No CI workflow runs found for current commit"
		log_info "CI checks may not have started yet. Waiting for workflow to appear..."
		return 2 # Special return code to indicate we should wait
	fi

	# Parse the workflow status
	local status conclusion run_id
	status=$(echo "$workflow_runs" | jq -r '.status // empty' | head -1)
	conclusion=$(echo "$workflow_runs" | jq -r '.conclusion // empty' | head -1)
	run_id=$(echo "$workflow_runs" | jq -r '.databaseId // empty' | head -1)

	log_info "CI workflow status: $status"
	[[ -n "$conclusion" ]] && log_info "CI workflow conclusion: $conclusion"

	case "$status" in
		"completed")
			case "$conclusion" in
				"success")
					log_success "✅ CI checks passed!"
					return 0
					;;
				"failure"|"cancelled"|"timed_out")
					log_error "❌ CI checks failed with conclusion: $conclusion"
					log_error "View details at: https://github.com/$GITHUB_REPO/actions/runs/$run_id"
					return 1
					;;
				*)
					log_warning "⚠️  CI completed with unknown conclusion: $conclusion"
					return 1
					;;
			esac
			;;
		"in_progress"|"queued"|"requested"|"waiting"|"pending")
			log_info "⏳ CI checks are currently running..."
			return 2 # Special return code to indicate we should wait
			;;
		*)
			log_warning "Unknown CI workflow status: $status"
			return 1
			;;
	esac
}

# Wait for CI workflow to complete
wait_for_ci_checks() {
	local start_time elapsed_time
	local no_workflow_wait_time=90 # Wait 90 seconds for workflow to appear

	start_time=$(date +%s)
	log_step "Waiting for CI checks to complete..."
	log_info "CI check timeout: ${CI_CHECK_TIMEOUT}s ($((CI_CHECK_TIMEOUT / 60)) minutes)"

	while true; do
		elapsed_time=$(($(date +%s) - start_time))

		# Check timeout
		if [[ $elapsed_time -gt $CI_CHECK_TIMEOUT ]]; then
			log_error "CI check timeout after ${CI_CHECK_TIMEOUT}s"
			return 1
		fi

		# Check CI status
		if check_ci_status; then
			echo # New line after progress indicator
			return 0
		else
			local check_result=$?
			if [[ $check_result -eq 2 ]]; then
				# If no workflow found and we've waited long enough, give up
				if [[ $elapsed_time -ge $no_workflow_wait_time ]]; then
					echo # New line after progress indicator
					log_warning "No CI workflow found after ${no_workflow_wait_time} seconds"
					return 3 # Special return code for "no workflow found"
				fi
				# Still waiting, check again
				printf "\r${BLUE}INFO:${NC} ⏳ Waiting for CI checks... (%ss elapsed)" ${elapsed_time}
				sleep $WORKFLOW_CHECK_INTERVAL
			else
				# Failed
				echo # New line after progress indicator
				return 1
			fi
		fi
	done
}

# Check and wait for CI to pass
ensure_ci_passes() {
	log_step "Validating CI status..."

	# First check current status
	if check_ci_status; then
		# CI already passed
		return 0
	else
		local check_result=$?
		if [[ $check_result -eq 2 ]]; then
			# CI is running, wait for it
			local wait_result
			wait_for_ci_checks
			wait_result=$?

			if [[ $wait_result -eq 0 ]]; then
				# CI passed
				return 0
			elif [[ $wait_result -eq 3 ]]; then
				# No workflow found after waiting
				log_warning "No CI workflow runs were found for the current commit"
				log_info "This may happen if:"
				log_info "  • CI hasn't been triggered yet for this commit"
				log_info "  • The commit was made directly without pushing to GitHub"
				log_info "  • CI workflow is not configured to run on the main branch"
				echo
				read -p "Do you want to proceed with the release anyway? (y/N): " -n 1 -r
				echo
				if [[ $REPLY =~ ^[Yy]$ ]]; then
					log_warning "Proceeding without CI validation"
					return 0
				else
					log_info "Release cancelled. Push your changes and wait for CI to run."
					return 1
				fi
			else
				# CI failed or timed out
				log_error "CI checks did not pass"
				log_error "Please fix the failing checks before creating a release"
				return 1
			fi
		else
			# CI failed
			log_error "CI checks have failed"
			log_error "Please fix the failing checks before creating a release"
			return 1
		fi
	fi
}

# Clean up local and remote tags
cleanup_tag() {
	local tag="$1"
	local cleanup_remote="${2:-true}"

	log_warning "Cleaning up tag: $tag"

	# Remove local tag if it exists
	if git tag -l | grep -q "^${tag}$"; then
		log_info "Removing local tag: $tag"
		git tag -d "$tag" || log_warning "Failed to remove local tag: $tag"
	fi

	# Remove remote tag if it exists and cleanup_remote is true
	if [[ "$cleanup_remote" == "true" ]]; then
		log_info "Checking if remote tag exists: $tag"
		if git ls-remote --tags origin | grep -q "refs/tags/${tag}$"; then
			log_info "Removing remote tag: $tag"
			git push --delete origin "$tag" || log_warning "Failed to remove remote tag: $tag"
		fi
	fi
}

# Wait for GitHub Actions workflow to complete
wait_for_workflow() {
	local tag="$1"
	local start_time elapsed_time

	start_time=$(date +%s)
	log_step "Monitoring GitHub Actions workflow for tag: $tag"
	log_info "Workflow timeout: ${WORKFLOW_TIMEOUT}s ($((WORKFLOW_TIMEOUT / 60)) minutes)"

	# Wait a bit for the workflow to start
	log_info "Waiting for workflow to start..."
	sleep 10

	while true; do
		elapsed_time=$(($(date +%s) - start_time))

		# Check timeout
		if [[ $elapsed_time -gt $WORKFLOW_TIMEOUT ]]; then
			log_error "Workflow timeout after ${WORKFLOW_TIMEOUT}s"
			return 1
		fi

		# Get workflow runs for this tag
		local workflow_status
		workflow_status=$(gh run list \
			--repo="$GITHUB_REPO" \
			--workflow="$WORKFLOW_FILE" \
			--event=push \
			--limit=5 \
			--json=status,conclusion,headBranch,headSha,event \
			--jq ".[] | select(.headBranch == \"$tag\" or (.event == \"push\" and .headSha != null)) | {status: .status, conclusion: .conclusion}" \
			2>/dev/null | head -1)

		if [[ -n "$workflow_status" ]]; then
			local status conclusion
			status=$(echo "$workflow_status" | jq -r '.status // empty')
			conclusion=$(echo "$workflow_status" | jq -r '.conclusion // empty')

			case "$status" in
			"completed")
				case "$conclusion" in
				"success")
					log_success "✅ GitHub Actions workflow completed successfully!"
					log_info "Release should be available at: https://github.com/$GITHUB_REPO/releases/tag/$tag"
					return 0
					;;
				"failure" | "cancelled" | "timed_out")
					log_error "❌ GitHub Actions workflow failed with conclusion: $conclusion"
					log_error "Check workflow logs at: https://github.com/$GITHUB_REPO/actions"
					return 1
					;;
				*)
					log_warning "⚠️  Workflow completed with unknown conclusion: $conclusion"
					return 1
					;;
				esac
				;;
			"in_progress" | "queued" | "requested" | "waiting" | "pending")
				printf "\r${BLUE}INFO:${NC} ⏳ Workflow status: $status (%ss elapsed)" ${elapsed_time}
				;;
			*)
				log_warning "Unknown workflow status: $status"
				;;
			esac
		else
			printf "\r${BLUE}INFO:${NC} 🔍 Looking for workflow... (%ss elapsed)" ${elapsed_time}
		fi

		sleep $WORKFLOW_CHECK_INTERVAL
	done
}

# Rollback version changes
rollback_version() {
	log_warning "Rolling back version changes..."

	# Reset to HEAD (before the version commit)
	if git diff --cached --quiet && git diff --quiet; then
		# If there are no changes, reset to previous commit
		git reset --hard HEAD~1 2>/dev/null || log_warning "Could not rollback version commit"
	else
		# If there are uncommitted changes, just reset the files
		git checkout HEAD -- Cargo.toml crates/mcptools/Cargo.toml Cargo.lock 2>/dev/null || log_warning "Could not reset Cargo.toml files"
	fi
}

# Update version in Cargo.toml files
update_version() {
	local new_version="$1"

	log_info "Updating version to $new_version in Cargo.toml files..."

	# Create backup
	cp "$PROJECT_ROOT/Cargo.toml" "$PROJECT_ROOT/Cargo.toml.backup"
	cp "$PROJECT_ROOT/crates/mcptools/Cargo.toml" "$PROJECT_ROOT/crates/mcptools/Cargo.toml.backup"

	# Update root Cargo.toml
	sed -i.bak "s/^version = \".*\"/version = \"$new_version\"/" "$PROJECT_ROOT/Cargo.toml"

	# Update crates/mcptools/Cargo.toml
	sed -i.bak "s/^version = \".*\"/version = \"$new_version\"/" "$PROJECT_ROOT/crates/mcptools/Cargo.toml"

	# Remove backup files created by sed
	rm -f "$PROJECT_ROOT/Cargo.toml.bak" "$PROJECT_ROOT/crates/mcptools/Cargo.toml.bak"

	# Update Cargo.lock
	log_info "Updating Cargo.lock..."
	cd "$PROJECT_ROOT"
	if ! cargo check --quiet; then
		log_error "cargo check failed after version update"
		# Restore from backup
		mv "$PROJECT_ROOT/Cargo.toml.backup" "$PROJECT_ROOT/Cargo.toml" 2>/dev/null || true
		mv "$PROJECT_ROOT/crates/mcptools/Cargo.toml.backup" "$PROJECT_ROOT/crates/mcptools/Cargo.toml" 2>/dev/null || true
		exit 1
	fi

	# Remove backups
	rm -f "$PROJECT_ROOT/Cargo.toml.backup" "$PROJECT_ROOT/crates/mcptools/Cargo.toml.backup"
}

# Create and push git tag with failure recovery
create_and_push_tag() {
	local version="$1"
	local tag="v$version"
	local monitor="${2:-true}"

	# Clean up any existing tag first
	cleanup_tag "$tag"

	log_step "Creating git commit for version $version..."
	git add Cargo.toml crates/mcptools/Cargo.toml Cargo.lock
	git commit -m "chore: bump version to $version"

	log_step "Creating git tag $tag..."
	git tag -a "$tag" -m "Release $version"

	log_step "Pushing changes and tag to origin..."
	git push origin main
	git push origin "$tag"

	log_success "✅ Tag $tag created and pushed successfully!"

	if [[ "$monitor" == "true" ]]; then
		log_info "Monitoring GitHub Actions workflow..."
		log_info "You can also monitor at: https://github.com/$GITHUB_REPO/actions"

		if wait_for_workflow "$tag"; then
			echo
			log_success "🎉 Release $version completed successfully!"
			return 0
		else
			log_error "💥 GitHub Actions workflow failed!"
			return 1
		fi
	else
		log_info "Skipping workflow monitoring. Check status at: https://github.com/$GITHUB_REPO/actions"
		return 0
	fi
}

# Cleanup after failure
cleanup_after_failure() {
	local version="$1"
	local tag="v$version"

	log_error "Release process failed. Starting cleanup..."

	# Cleanup tags
	cleanup_tag "$tag" true

	# Rollback version changes
	rollback_version

	log_warning "Cleanup completed. You can now fix the issues and try again."
}

# Retry mechanism
retry_release() {
	local version="$1"
	local max_retries=3
	local retry_count=0

	# Set globals for cleanup handler
	CURRENT_VERSION="$version"
	CURRENT_TAG="v$version"

	while [[ $retry_count -lt $max_retries ]]; do
		if [[ $retry_count -gt 0 ]]; then
			log_warning "Retry attempt $retry_count of $max_retries"
			echo
			read -p "Do you want to retry the release? (y/N): " -n 1 -r
			echo
			if [[ ! $REPLY =~ ^[Yy]$ ]]; then
				log_info "Release cancelled by user."
				exit 0
			fi
		fi

		if create_and_push_tag "$version" true; then
			return 0
		else
			((retry_count++))
			log_error "Release attempt $retry_count failed."

			if [[ $retry_count -lt $max_retries ]]; then
				cleanup_after_failure "$version"
				log_info "Cleaned up failed release. Ready for retry."
				echo
			fi
		fi
	done

	log_error "All $max_retries release attempts failed."
	cleanup_after_failure "$version"
	exit 1
}

# Main function
main() {
	local new_version="$1"

	cd "$PROJECT_ROOT"

	log_step "Starting release process for version $new_version..."
	echo

	# Validations
	log_step "Running pre-release checks..."
	check_gh_cli
	check_main_branch
	check_clean_working_dir
	validate_version "$new_version"

	# Check CI status
	if ! ensure_ci_passes; then
		log_error "Cannot proceed with release until CI checks pass"
		exit 1
	fi
	echo

	# Get current version
	local current_version
	current_version=$(get_current_version)
	log_info "Current version: $current_version"
	log_info "New version: $new_version"

	# Show what will happen
	echo
	log_step "Release Plan:"
	echo "  1. Update version in Cargo.toml files"
	echo "  2. Create git commit and tag v$new_version"
	echo "  3. Push changes and tag to GitHub"
	echo "  4. Monitor GitHub Actions release workflow"
	echo "  5. Verify release creation"
	echo "  6. If workflow fails: cleanup and offer retry"
	echo

	# Confirm with user
	read -p "Are you sure you want to release version $new_version? (y/N): " -n 1 -r
	echo
	if [[ ! $REPLY =~ ^[Yy]$ ]]; then
		log_info "Release cancelled."
		exit 0
	fi

	# Perform release with retry mechanism
	update_version "$new_version"
	retry_release "$new_version"

	echo
	log_success "🎉 Release $new_version completed successfully!"
	log_info "📦 Release available at: https://github.com/$GITHUB_REPO/releases/tag/v$new_version"
	log_info "📋 Installation instructions are included in the release notes."

	# Test the upgrade command if mcptools is installed
	if command -v mcptools >/dev/null 2>&1; then
		local should_upgrade=false

		if [[ "$AUTO_UPGRADE" == "true" ]]; then
			should_upgrade=true
		else
			echo
			read -p "Would you like to upgrade your local mcptools binary to the new version? (y/N): " -n 1 -r
			echo
			if [[ $REPLY =~ ^[Yy]$ ]]; then
				should_upgrade=true
			fi
		fi

		if [[ "$should_upgrade" == "true" ]]; then
			log_step "Testing the upgrade command..."
			log_info "Running 'mcptools upgrade' to verify the release works correctly"
			if mcptools upgrade; then
				log_success "✅ Upgrade command executed successfully!"
				log_info "Your mcptools binary has been updated to version $new_version"
			else
				log_warning "⚠️  Upgrade command failed, but the release was created successfully"
				log_info "You can still download the release manually from GitHub"
			fi
		else
			log_info "💡 You can upgrade later by running: mcptools upgrade"
		fi
	else
		log_info "💡 Tip: Install mcptools to test releases with 'mcptools upgrade'"
	fi
}

# Script usage
usage() {
	echo "MCPTOOLS DevOps CLI Release Script with Failure Recovery"
	echo
	echo "Usage: $0 <version> [options]"
	echo
	echo "Arguments:"
	echo "  <version>           Semantic version number (e.g., 1.0.0, 2.1.0-beta.1)"
	echo
	echo "Options:"
	echo "  -h, --help         Show this help message"
	echo "  --cleanup <tag>    Clean up failed release tag (e.g., --cleanup v1.0.0)"
	echo "  --upgrade          Automatically upgrade local mcptools binary after successful release"
	echo
	echo "Examples:"
	echo "  $0 1.0.0                    # Create release v1.0.0"
	echo "  $0 1.0.0 --upgrade          # Create release and auto-upgrade"
	echo "  $0 1.0.0-beta.1             # Create pre-release"
	echo "  $0 --cleanup v1.0.0         # Clean up failed v1.0.0 release"
	echo
	echo "Features:"
	echo "  • Automated GitHub Actions workflow monitoring"
	echo "  • Automatic cleanup and retry on failure"
	echo "  • Tag cleanup (local and remote)"
	echo "  • Version rollback on failure"
	echo "  • Pre-release validation checks"
	echo
	echo "Requirements:"
	echo "  • GitHub CLI (gh) installed and authenticated"
	echo "  • Clean working directory on main branch"
	echo "  • Valid semantic version number"
	echo
	echo "This script will:"
	echo "  1. Validate prerequisites (branch, auth, working dir)"
	echo "  2. Check CI status and wait for checks to pass"
	echo "  3. Update version in Cargo.toml files"
	echo "  4. Create git commit and tag"
	echo "  5. Push changes to trigger GitHub Actions"
	echo "  6. Monitor workflow progress and status"
	echo "  7. On failure: cleanup tags and offer retry (up to 3 attempts)"
	echo "  8. On success: show release URL and installation instructions"
	echo "  9. Optionally upgrade local mcptools binary (with prompt or --upgrade flag)"
}

# Handle cleanup command
cleanup_command() {
	local tag="$1"

	cd "$PROJECT_ROOT"

	log_step "Cleaning up failed release: $tag"

	# Validate tag format
	if [[ ! $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$ ]]; then
		log_error "Invalid tag format: $tag. Expected format: v1.0.0"
		exit 1
	fi

	# Extract version from tag
	local version="${tag#v}"

	# Confirm with user
	echo
	log_warning "This will:"
	echo "  • Remove local tag: $tag"
	echo "  • Remove remote tag: $tag (if exists)"
	echo "  • Rollback version changes (if any)"
	echo
	read -p "Are you sure you want to cleanup $tag? (y/N): " -n 1 -r
	echo
	if [[ ! $REPLY =~ ^[Yy]$ ]]; then
		log_info "Cleanup cancelled."
		exit 0
	fi

	cleanup_after_failure "$version"
	log_success "✅ Cleanup completed for $tag"
}

# Parse command line arguments
VERSION=""
while [[ $# -gt 0 ]]; do
	case $1 in
	-h | --help)
		usage
		exit 0
		;;
	--cleanup)
		if [[ $# -lt 2 ]]; then
			log_error "Missing tag argument for --cleanup"
			usage
			exit 1
		fi
		check_gh_cli
		cleanup_command "$2"
		exit 0
		;;
	--upgrade)
		AUTO_UPGRADE=true
		shift
		;;
	-*)
		log_error "Unknown option: $1"
		usage
		exit 1
		;;
	*)
		# This is the version argument
		if [[ -n "$VERSION" ]]; then
			log_error "Version already specified: $VERSION"
			usage
			exit 1
		fi
		VERSION="$1"
		shift
		;;
	esac
done

# Check if version was provided
if [[ -z "$VERSION" ]]; then
	log_error "No version specified"
	usage
	exit 1
fi

# Run main with the version
main "$VERSION"
exit 0
