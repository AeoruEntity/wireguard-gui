#!/bin/bash
set -e

VERSION="0.1.0"
PKG_NAME="aeoru-vpn"
ARCH="amd64"
APP_ID="com.aeoru.nvr"
ICON_SRC="data/aeoru-nvr-icon.png"
DESKTOP_SRC="data/com.aeoru.nvr.desktop"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BUILD_DIR="$SCRIPT_DIR/build-deb-tmp"
OUTPUT="$SCRIPT_DIR/${PKG_NAME}_${VERSION}_${ARCH}.deb"

echo "==> Building release binary..."
cargo build --release

echo "==> Preparing .deb structure..."
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR/usr/bin"
mkdir -p "$BUILD_DIR/usr/share/applications"

# Copy DEBIAN control files
cp -r "$SCRIPT_DIR/pkg/DEBIAN" "$BUILD_DIR/DEBIAN"
chmod 755 "$BUILD_DIR/DEBIAN/postinst" "$BUILD_DIR/DEBIAN/postrm"

# Binary
cp "$SCRIPT_DIR/target/release/aeoru-nvr" "$BUILD_DIR/usr/bin/aeoru-nvr"
chmod 755 "$BUILD_DIR/usr/bin/aeoru-nvr"

# Desktop file
cp "$SCRIPT_DIR/$DESKTOP_SRC" "$BUILD_DIR/usr/share/applications/$APP_ID.desktop"

# Icons - resize source icon to all standard sizes
# Requires ImageMagick (convert/magick). Falls back to copying original if not available.
ICON_SIZES="16 24 32 48 64 128 256"
if command -v convert &>/dev/null; then
    for size in $ICON_SIZES; do
        dir="$BUILD_DIR/usr/share/icons/hicolor/${size}x${size}/apps"
        mkdir -p "$dir"
        convert "$SCRIPT_DIR/$ICON_SRC" -resize "${size}x${size}" "$dir/$APP_ID.png"
    done
elif command -v magick &>/dev/null; then
    for size in $ICON_SIZES; do
        dir="$BUILD_DIR/usr/share/icons/hicolor/${size}x${size}/apps"
        mkdir -p "$dir"
        magick "$SCRIPT_DIR/$ICON_SRC" -resize "${size}x${size}" "$dir/$APP_ID.png"
    done
else
    echo "WARNING: ImageMagick not found. Copying original icon to all sizes."
    for size in $ICON_SIZES; do
        dir="$BUILD_DIR/usr/share/icons/hicolor/${size}x${size}/apps"
        mkdir -p "$dir"
        cp "$SCRIPT_DIR/$ICON_SRC" "$dir/$APP_ID.png"
    done
fi

# Update installed size in control file
INSTALLED_KB=$(du -sk "$BUILD_DIR" | cut -f1)
sed -i "s/^Installed-Size:.*/Installed-Size: $INSTALLED_KB/" "$BUILD_DIR/DEBIAN/control"

echo "==> Building .deb package..."
dpkg-deb --build "$BUILD_DIR" "$OUTPUT"

# Clean up
rm -rf "$BUILD_DIR"

echo ""
echo "==> Built: $OUTPUT"
echo ""
echo "Install with:  sudo dpkg -i $OUTPUT"
echo "Remove with:   sudo dpkg -r $PKG_NAME"
