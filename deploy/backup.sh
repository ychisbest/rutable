#!/bin/sh
set -eu
umask 077
mkdir -p /var/backups/rutable
sqlite3 /var/lib/rutable/rutable.sqlite3 ".backup '/var/backups/rutable/rutable-$(date -u +%Y%m%d-%H%M%S).sqlite3'"
find /var/backups/rutable -name 'rutable-*.sqlite3' -mtime +14 -delete
