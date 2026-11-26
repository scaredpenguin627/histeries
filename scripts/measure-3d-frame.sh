#!/usr/bin/env bash
# Compare captures from the same viewport, excluding inspector, menus and labels.
set -euo pipefail
reference=${1:?expected a reference frame}
scene=${2:?expected a 3D frame}
read -r width height <<<"$(identify -format '%w %h' "$scene")"
[[ $(identify -format '%w %h' "$reference") == "$width $height" ]]
crop="$((width * 68 / 100))x$((height * 68 / 100))+$((width / 25))+$((height * 16 / 100))"
temp=$(mktemp -d)
trap 'rm -rf "$temp"' EXIT
sample() {
    convert "$1" -crop "$crop" +repage -sample '160x100!' -alpha off -colorspace sRGB -depth 8 "$2"
}
sample "$reference" "$temp/reference.miff"
sample "$scene" "$temp/scene.miff"
changed=$(convert "$temp/reference.miff" "$temp/scene.miff" -compose difference -composite \
    -colorspace Gray -threshold 5% -format '%[fx:mean]' info:)
# Count repeatedly drawn material colors. A blank canvas or dark fog alone cannot pass.
materials=$(convert "$temp/scene.miff" -depth 8 rgb:- | od -An -v -tu1 -w3 | awk '
    NF == 3 {
        hi = lo = $1
        for (i=2; i<=3; ++i) { if ($i > hi) hi=$i; if ($i < lo) lo=$i }
        if (hi > 70 && hi-lo > 18) {
            ++surface
            ++bands[int($1/16) ":" int($2/16) ":" int($3/16)]
        }
    }
    END {
        for (b in bands) if (bands[b] >= 16) ++repeated
        print surface+0, repeated+0
    }')
read -r surface bands <<<"$materials"
jq -n --argjson width "$width" --argjson height "$height" --argjson changed "$changed" \
    --argjson surface "$surface" --argjson bands "$bands" \
    '{width:$width, height:$height, sampled_pixels:16000, changed_fraction:$changed,
      colored_surface_samples:$surface, recurring_color_bands:$bands}'
awk -v changed="$changed" -v surface="$surface" -v bands="$bands" \
    'BEGIN { exit !(changed >= 0.02 && surface >= 320 && bands >= 4) }'
