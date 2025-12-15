#!/bin/bash

# Publish script for statsig-client to crates.io
# This script handles the complete publishing process including version bumping and validation

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if we're on the main branch
check_branch() {
    local current_branch=$(git rev-parse --abbrev-ref HEAD)
    if [[ "$current_branch" != "main" ]]; then
        print_error "Must be on main branch to publish. Current branch: $current_branch"
        exit 1
    fi
    print_status "On main branch ✓"
}

# Check if working directory is clean
check_clean_working_dir() {
    if [[ -n $(git status --porcelain) ]]; then
        print_error "Working directory is not clean. Please commit or stash changes."
        exit 1
    fi
    print_status "Working directory is clean ✓"
}

# Check if cargo is installed and configured
check_cargo_config() {
    if ! command -v cargo &> /dev/null; then
        print_error "cargo is not installed"
        exit 1
    fi

    # Check if we have a crates.io token
    if ! cargo login --help &> /dev/null; then
        print_error "cargo login command not available"
        exit 1
    fi

    print_status "Cargo is available ✓"
}

# Run tests and checks
run_checks() {
    print_status "Running tests and checks..."
    
    # Run tests
    if ! cargo test --all-features; then
        print_error "Tests failed"
        exit 1
    fi
    print_status "Tests passed ✓"

    # Check formatting
    if ! cargo fmt --all -- --check; then
        print_error "Code formatting check failed"
        exit 1
    fi
    print_status "Code formatting check passed ✓"

    # Run clippy
    if ! cargo clippy --all-targets --all-features -- -D warnings; then
        print_error "Clippy check failed"
        exit 1
    fi
    print_status "Clippy check passed ✓"

    # Check documentation
    if ! cargo doc --no-deps --all-features; then
        print_error "Documentation check failed"
        exit 1
    fi
    print_status "Documentation check passed ✓"
}

# Bump version
bump_version() {
    local version_type=$1
    if [[ -z "$version_type" ]]; then
        print_warning "No version type specified, defaulting to patch"
        version_type="patch"
    fi

    print_status "Bumping $version_type version..."
    
    # Use cargo-setuptools to bump version if available, otherwise manual
    if command -v cargo-edit &> /dev/null; then
        cargo set-version --bump $version_type
    else
        print_warning "cargo-edit not found, please install it with: cargo install cargo-edit"
        print_warning "You'll need to manually bump the version in Cargo.toml"
        read -p "Press Enter to continue after manually bumping version..."
    fi

    local new_version=$(grep '^version = ' Cargo.toml | sed 's/version = "//' | sed 's/"//')
    print_status "New version: $new_version"
}

# Create git tag and push
create_git_tag() {
    local version=$(grep '^version = ' Cargo.toml | sed 's/version = "//' | sed 's/"//')
    local tag_name="v$version"

    print_status "Creating git tag: $tag_name"
    
    # Add and commit the version change
    git add Cargo.toml
    git commit -m "Bump version to $version"
    
    # Create tag
    git tag -a "$tag_name" -m "Release $version"
    
    # Push to remote
    git push origin main
    git push origin "$tag_name"
    
    print_status "Git tag created and pushed ✓"
}

# Publish to crates.io
publish_to_crates() {
    print_status "Publishing to crates.io..."
    
    # Dry run first
    print_status "Running publish dry run..."
    if ! cargo publish --dry-run; then
        print_error "Publish dry run failed"
        exit 1
    fi
    print_status "Publish dry run passed ✓"

    # Actual publish
    print_status "Publishing to crates.io..."
    if ! cargo publish; then
        print_error "Publish failed"
        exit 1
    fi
    
    print_status "Successfully published to crates.io! ✓"
}

# Main function
main() {
    local version_type=$1
    
    print_status "Starting publish process for statsig-client..."
    
    check_branch
    check_clean_working_dir
    check_cargo_config
    run_checks
    
    if [[ -n "$version_type" ]]; then
        bump_version "$version_type"
        create_git_tag
    fi
    
    publish_to_crates
    
    print_status "Publish process completed successfully! 🎉"
}

# Show usage
usage() {
    echo "Usage: $0 [version_type]"
    echo "  version_type: major, minor, or patch (optional)"
    echo ""
    echo "Examples:"
    echo "  $0 patch    # Bump patch version and publish"
    echo "  $0 minor    # Bump minor version and publish"
    echo "  $0          # Publish current version without bumping"
}

# Check for help flag
if [[ "$1" == "-h" || "$1" == "--help" ]]; then
    usage
    exit 0
fi

# Run main function
main "$@"