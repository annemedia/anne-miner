#!/bin/bash
# mac_build.sh - Build for both Intel and Apple Silicon with CLI, .app bundle and DMG

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_step() {
    echo -e "\n${BLUE}==>${NC} ${GREEN}$1${NC}"
}

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

check_tools() {
    local missing_tools=()
    
    for tool in cargo lipo hdiutil sips iconutil; do
        if ! command -v $tool &> /dev/null; then
            missing_tools+=($tool)
        fi
    done
    
    if [ ${#missing_tools[@]} -gt 0 ]; then
        print_error "Missing required tools: ${missing_tools[*]}"
        print_info "Please install missing tools and try again"
        exit 1
    fi
}


cleanup() {
    print_step "Cleaning previous builds..."
    cargo clean
    
    rm -rf target/universal-apple-darwin/release
    mkdir -p target/universal-apple-darwin/release
}

build_target() {
    local target=$1
    local features=$2
    
    print_step "Building for $target with features: $features..."
    
    if [ "$target" = "x86_64-apple-darwin" ]; then
        export MACOSX_DEPLOYMENT_TARGET=10.12
        print_info "Using MACOSX_DEPLOYMENT_TARGET=10.12"
    else
        export MACOSX_DEPLOYMENT_TARGET=11.0
        print_info "Using MACOSX_DEPLOYMENT_TARGET=11.0"
    fi
    
    export NO_MTUNE_NATIVE=1
    print_info "Set NO_MTUNE_NATIVE=1 to prevent -mtune=native flag"
    

    cargo build --release --target $target --features "$features"
    
    local build_status=$?
    
    unset MACOSX_DEPLOYMENT_TARGET
    unset NO_MTUNE_NATIVE
    
    if [ $build_status -ne 0 ]; then
        print_error "$target build failed!"
        exit 1
    fi
}

create_universal_binary() {
    local intel_binary=$1
    local silicon_binary=$2
    local output_name=$3
    
    print_step "Creating universal binary: $output_name..."
    
    lipo -create \
        target/x86_64-apple-darwin/release/anne-miner \
        target/aarch64-apple-darwin/release/anne-miner \
        -output $output_name
    
    if [ $? -eq 0 ]; then
        print_info "Universal binary created successfully: $output_name"
        chmod +x $output_name
        lipo -info $output_name
    else
        print_error "Failed to create universal binary: $output_name"
        exit 1
    fi
}

create_app_bundle() {
    local gui_binary=$1
    
    print_step "Creating GUI .app bundle..."
    
    mkdir -p "target/universal-apple-darwin/release/Anne Miner.app/Contents/MacOS"
    mkdir -p "target/universal-apple-darwin/release/Anne Miner.app/Contents/Resources"
    
    cp "$gui_binary" "target/universal-apple-darwin/release/Anne Miner.app/Contents/MacOS/anne-miner"
    chmod +x "target/universal-apple-darwin/release/Anne Miner.app/Contents/MacOS/anne-miner"
    
    cat > "target/universal-apple-darwin/release/Anne Miner.app/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>anne-miner</string>
    <key>CFBundleIdentifier</key>
    <string>media.anne.miner</string>
    <key>CFBundleName</key>
    <string>Anne Miner</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon.icns</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF
    
    if [ -f "assets/anneminer.png" ]; then
        print_info "Creating AppIcon.icns from anneminer.png..."
        mkdir -p temp_icon.iconset
        
        sips -z 16 16 assets/anneminer.png --out temp_icon.iconset/icon_16x16.png 2>/dev/null
        sips -z 32 32 assets/anneminer.png --out temp_icon.iconset/icon_16x16@2x.png 2>/dev/null
        sips -z 32 32 assets/anneminer.png --out temp_icon.iconset/icon_32x32.png 2>/dev/null
        sips -z 64 64 assets/anneminer.png --out temp_icon.iconset/icon_32x32@2x.png 2>/dev/null
        sips -z 128 128 assets/anneminer.png --out temp_icon.iconset/icon_128x128.png 2>/dev/null
        sips -z 256 256 assets/anneminer.png --out temp_icon.iconset/icon_128x128@2x.png 2>/dev/null
        sips -z 256 256 assets/anneminer.png --out temp_icon.iconset/icon_256x256.png 2>/dev/null
        sips -z 512 512 assets/anneminer.png --out temp_icon.iconset/icon_256x256@2x.png 2>/dev/null
        sips -z 512 512 assets/anneminer.png --out temp_icon.iconset/icon_512x512.png 2>/dev/null
        
        iconutil -c icns temp_icon.iconset -o "target/universal-apple-darwin/release/Anne Miner.app/Contents/Resources/AppIcon.icns" 2>/dev/null
        
        rm -rf temp_icon.iconset
        
        if [ -f "target/universal-apple-darwin/release/Anne Miner.app/Contents/Resources/AppIcon.icns" ]; then
            print_info "App icon created successfully"
        else
            print_warning "Failed to create AppIcon.icns"
        fi
    else
        print_warning "assets/anneminer.png not found. App bundle will have no custom icon."
    fi
    
    print_info "CLI .app bundle created: Anne Miner.app"
}

create_dmg() {
    print_step "Creating DMG..."
    
    mkdir -p target/universal-apple-darwin/release/dmg_contents
    cp -R "target/universal-apple-darwin/release/Anne Miner.app" target/universal-apple-darwin/release/dmg_contents/
    ln -s /Applications target/universal-apple-darwin/release/dmg_contents/Applications
    
    hdiutil create -volname "Anne Miner" \
                   -srcfolder target/universal-apple-darwin/release/dmg_contents \
                   -ov \
                   -format UDZO \
                   -imagekey zlib-level=9 \
                   target/universal-apple-darwin/release/anne-miner.dmg 2>/dev/null
    
    if [ $? -eq 0 ]; then
        print_info "DMG created successfully: target/universal-apple-darwin/release/anne-miner.dmg"
        
        print_info "DMG information:"
        hdiutil imageinfo target/universal-apple-darwin/release/anne-miner.dmg | grep -E "(format|size|Checksum)" | head -5
    else
        print_error "Failed to create DMG"
        exit 1
    fi
    
    rm -rf target/universal-apple-darwin/release/dmg_contents
}

main() {
    print_step "Starting macOS Universal Build Process"
    
    check_tools
    
    cleanup
    
    build_target "x86_64-apple-darwin" "opencl"
    build_target "aarch64-apple-darwin" "opencl"
    
    create_universal_binary \
        target/x86_64-apple-darwin/release/anne-miner \
        target/aarch64-apple-darwin/release/anne-miner \
        target/universal-apple-darwin/release/anne-miner
    
    create_app_bundle "target/universal-apple-darwin/release/anne-miner"
    
    create_dmg
    
    print_step "Build completed successfully!"
    echo ""
    echo "Generated artifacts:"
    echo "  • CLI Binary:        target/universal-apple-darwin/anne-miner"
    echo "  • App Bundle:        target/universal-apple-darwin/Anne Miner.app"
    echo "  • Installer DMG:     target/universal-apple-darwin/anne-miner.dmg"
    echo ""
    echo "File sizes:"
    ls -lh target/universal-apple-darwin/release/anne-miner target/universal-apple-darwin/release/anne-miner.dmg | awk '{print $5, $9}'
    du -sh "target/universal-apple-darwin/release/Anne Miner.app"
}

main