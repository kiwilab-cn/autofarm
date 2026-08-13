#!/bin/bash
set -euo pipefail

project_root="$(cd "$(dirname "$0")/.." && pwd)"
source_dir="$project_root/source-assets/maps/verdant-paddy"
output_dir="$project_root/assets/art/pixel/tilesets"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

mkdir -p "$output_dir"

terrain_source="$source_dir/terrain-atlas-source.png"
terrain_output="$output_dir/verdant-paddy-terrain.png"
facility_source="$source_dir/facility-atlas-source.png"
facility_output="$output_dir/verdant-paddy-facilities.png"
infrastructure_source="$source_dir/infrastructure-atlas-source.png"
infrastructure_output="$output_dir/verdant-paddy-infrastructure.png"

for row in 0 1 2 3; do
    for column in 0 1 2 3; do
        index=$((row * 4 + column))
        x=$((column * 313 + 4))
        y=$((row * 313 + 4))
        ffmpeg -hide_banner -loglevel error -y \
            -i "$terrain_source" \
            -vf "crop=305:305:$x:$y,scale=32:32:flags=area" \
            "$work_dir/terrain-$index.png"
    done
done

ffmpeg -hide_banner -loglevel error -y \
    -i "$work_dir/terrain-0.png" -i "$work_dir/terrain-1.png" \
    -i "$work_dir/terrain-2.png" -i "$work_dir/terrain-3.png" \
    -i "$work_dir/terrain-4.png" -i "$work_dir/terrain-5.png" \
    -i "$work_dir/terrain-6.png" -i "$work_dir/terrain-7.png" \
    -i "$work_dir/terrain-8.png" -i "$work_dir/terrain-9.png" \
    -i "$work_dir/terrain-10.png" -i "$work_dir/terrain-11.png" \
    -i "$work_dir/terrain-12.png" -i "$work_dir/terrain-13.png" \
    -i "$work_dir/terrain-14.png" -i "$work_dir/terrain-15.png" \
    -filter_complex \
    "[0:v][1:v][2:v][3:v]hstack=inputs=4[row0];[4:v][5:v][6:v][7:v]hstack=inputs=4[row1];[8:v][9:v][10:v][11:v]hstack=inputs=4[row2];[12:v][13:v][14:v][15:v]hstack=inputs=4[row3];[row0][row1][row2][row3]vstack=inputs=4" \
    "$terrain_output"

ffmpeg -hide_banner -loglevel error -y \
    -i "$facility_source" \
    -vf "colorkey=0xff00ff:0.38:0.0,format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='if(gt(min(r(X,Y),b(X,Y)),g(X,Y)*1.15+1),0,alpha(X,Y))',scale=768:768:flags=neighbor" \
    "$facility_output"

ffmpeg -hide_banner -loglevel error -y \
    -i "$infrastructure_source" \
    -vf "scale=128:64:flags=area" \
    "$infrastructure_output"

python3 "$project_root/tools/generate_verdant_paddy.py"

terrain_dimensions="$(sips -g pixelWidth -g pixelHeight "$terrain_output" 2>/dev/null | tr '\n' ' ')"
facility_dimensions="$(sips -g pixelWidth -g pixelHeight "$facility_output" 2>/dev/null | tr '\n' ' ')"
infrastructure_dimensions="$(sips -g pixelWidth -g pixelHeight "$infrastructure_output" 2>/dev/null | tr '\n' ' ')"

case "$terrain_dimensions" in
    *"pixelWidth: 128"*"pixelHeight: 128"*) ;;
    *) echo "invalid terrain atlas dimensions: $terrain_dimensions" >&2; exit 1 ;;
esac
case "$facility_dimensions" in
    *"pixelWidth: 768"*"pixelHeight: 768"*) ;;
    *) echo "invalid facility atlas dimensions: $facility_dimensions" >&2; exit 1 ;;
esac
case "$infrastructure_dimensions" in
    *"pixelWidth: 128"*"pixelHeight: 64"*) ;;
    *) echo "invalid infrastructure atlas dimensions: $infrastructure_dimensions" >&2; exit 1 ;;
esac

echo "built $terrain_output (128x128, 16 tiles)"
echo "built $facility_output (768x768, 9 sprites)"
echo "built $infrastructure_output (128x64, 8 tiles)"
echo "built Tiled authoring package under assets/maps"
