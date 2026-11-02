#!/bin/sh
# Create and trust a local self-signed code-signing identity named
# "Wuwa Tappo Local Signing" in the login keychain.
#
# Why: macOS ties the Input Monitoring grant to the app's code-signing
# requirement. Ad-hoc signatures change on every build, which silently
# revokes the grant; a stable (even self-signed) identity keeps it.
# Only needed once per machine. Skip this and set
# bundle.macOS.signingIdentity to "-" in src-tauri/tauri.conf.json if you
# prefer ad-hoc signing (re-grant Input Monitoring after each rebuild).
set -eu
NAME="Wuwa Tappo Local Signing"
if security find-identity -v -p codesigning | grep -q "$NAME"; then
  echo "identity already present"; exit 0
fi
DIR=$(mktemp -d)
cat > "$DIR/ext.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = v3
prompt = no
[dn]
CN = $NAME
[v3]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
subjectKeyIdentifier = hash
CNF
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -keyout "$DIR/key.pem" -out "$DIR/cert.pem" -config "$DIR/ext.cnf" 2>/dev/null
openssl pkcs12 -export -out "$DIR/id.p12" -inkey "$DIR/key.pem" -in "$DIR/cert.pem" -passout pass:local 2>/dev/null \
  || openssl pkcs12 -export -legacy -out "$DIR/id.p12" -inkey "$DIR/key.pem" -in "$DIR/cert.pem" -passout pass:local
security import "$DIR/id.p12" -k "$HOME/Library/Keychains/login.keychain-db" -P local -T /usr/bin/codesign -T /usr/bin/security
security add-trusted-cert -r trustRoot -p codeSign -k "$HOME/Library/Keychains/login.keychain-db" "$DIR/cert.pem"
rm -rf "$DIR"
security find-identity -v -p codesigning | grep "$NAME"
