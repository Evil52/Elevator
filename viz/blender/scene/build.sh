#!/bin/sh
set -eu
viz_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
export BLENDER_USER_RESOURCES="$viz_dir/.cache/blender"
mkdir -p "$BLENDER_USER_RESOURCES"
exec "${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}" \
  --factory-startup -b --python-exit-code 1 -P "$viz_dir/blender/scene/build.py" -- "$@"

