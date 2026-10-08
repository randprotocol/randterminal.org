#!/bin/sh
set -eu
/usr/local/bin/solc --standard-json --allow-paths /work < /work/input.json > /work/output.json
