#!/bin/sh
set -eu
binary=/build/target/release/pocket-tts-wyoming
root=/runtime-rootfs
mkdir -p "$root/usr/local/bin" "$root/etc/ssl/certs" "$root/app/models" "$root/app/voices" "$root/tmp"
cp "$binary" "$root/usr/local/bin/pocket-tts-wyoming"
# Copy the exact ELF dependency closure, including the dynamic interpreter.
ldd "$binary" > /build/runtime-ldd.txt
if grep -q 'not found' /build/runtime-ldd.txt; then exit 1; fi
awk '/=> \/ / {print $3} /=> \// {print $3} /^\s*\// {print $1}' /build/runtime-ldd.txt | sort -u > /build/runtime-libraries.txt
# POSIX awk does not interpret \s; explicitly include the ELF interpreter.
readelf -l "$binary" | sed -n 's/.*interpreter: \(.*\)]/\1/p' >> /build/runtime-libraries.txt
sort -u /build/runtime-libraries.txt -o /build/runtime-libraries.txt
while IFS= read -r library; do cp -L --parents "$library" "$root"; done < /build/runtime-libraries.txt
cp /etc/ssl/certs/ca-certificates.crt "$root/etc/ssl/certs/"
printf 'pockettts:x:99:100:Pocket TTS:/app:/sbin/nologin\n' > "$root/etc/passwd"
printf 'users:x:100:\n' > "$root/etc/group"
printf 'hosts: files dns\npasswd: files\ngroup: files\n' > "$root/etc/nsswitch.conf"
chmod 0755 "$root/app" "$root/app/models" "$root/app/voices"
chown 99:100 "$root/app" "$root/app/models" "$root/app/voices" "$root/tmp"
chmod 0700 "$root/tmp"

mkdir -p "$root/usr/share/licenses/misaki"
cp /build/licenses/misaki-APACHE-2.0.txt "$root/usr/share/licenses/misaki/LICENSE"
printf "%s\n" "Pure German normalization adapted to Rust from semidark/misaki bbaf917e7bf7fbbe830f74f4acda6583ce1c0caf; Nico Thomaier and apples-kksk; Apache-2.0." > "$root/usr/share/licenses/misaki/ATTRIBUTION"

# Preserve own and third-party license records in binary distributions.
mkdir -p "$root/usr/share/licenses/pocket-tts" "$root/usr/share/licenses/xn-ptts" "$root/usr/share/doc" "$root/usr/share/common-licenses"
cp /build/LICENSE /build/THIRD_PARTY_NOTICES.md "$root/usr/share/licenses/pocket-tts/"
cp /build/licenses/kyutai-MIT.txt "$root/usr/share/licenses/pocket-tts/"
cp /build/vendor/xn-ptts/LICENSE-MIT /build/vendor/xn-ptts/LICENSE-APACHE "$root/usr/share/licenses/xn-ptts/"
for package in libc6 libgcc-s1 gcc-12-base ca-certificates; do
  mkdir -p "$root/usr/share/doc/$package"
  cp "/usr/share/doc/$package/copyright" "$root/usr/share/doc/$package/"
done
cp /usr/share/common-licenses/* "$root/usr/share/common-licenses/"
dpkg-query -W -f='${binary:Package} ${Version} ${source:Package} ${source:Version}\n' libc6 libgcc-s1 gcc-12-base ca-certificates > "$root/usr/share/licenses/pocket-tts/debian-runtime-packages.txt"
# Cargo license inventory and unmodified upstream notices (no dependency code).
mkdir -p "$root/usr/share/licenses/cargo"
cargo tree --locked --target x86_64-unknown-linux-gnu --prefix none --format '{p} {l}' > "$root/usr/share/licenses/cargo/packages.txt"
for registry in "${CARGO_HOME:-/usr/local/cargo}"/registry/src/*; do
  [ -d "$registry" ] || continue
  for package in "$registry"/*; do
    [ -d "$package" ] || continue
    destination="$root/usr/share/licenses/cargo/$(basename "$package")"
    for notice in "$package"/LICENSE* "$package"/LICENCE* "$package"/COPYING* "$package"/NOTICE*; do
      [ -f "$notice" ] || continue
      mkdir -p "$destination"
      cp "$notice" "$destination/"
    done
  done
done

mkdir -p "$root/usr/share/licenses/pocket-tts/upstream"
cp /build/licenses/* "$root/usr/share/licenses/pocket-tts/upstream/"
