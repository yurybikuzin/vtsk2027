#!/bin/bash
# =============================================================================
# tree.sh – Display directory tree honouring nested .gitignore files
# =============================================================================
#
# USAGE:
#   ./tree.sh <directory>
#
# Reads <directory>/.gitignore and any nested .gitignore files encountered
# during traversal, and skips matching files/directories. The .gitignore files
# themselves are hidden from output.
# =============================================================================

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ignore-lib.sh
. "$SCRIPT_DIR/ignore-lib.sh"

ROOT_DIR="${1:-}"
if [ -z "$ROOT_DIR" ]; then
    echo "Error: Directory path is required." >&2
    echo "Usage: $0 <directory>" >&2
    exit 1
fi
if [ ! -d "$ROOT_DIR" ]; then
    echo "Error: '$ROOT_DIR' is not a valid directory." >&2
    exit 1
fi
ROOT_DIR="$(cd "$ROOT_DIR" && pwd)"

ignore_load "$ROOT_DIR"

ignore_push "$ROOT_DIR" "rs"
is_ignored "rs/target" 1; echo "rs/target ignored: $?" >&2; ignore_pop
 

list_dir() {
    local dir
    dir="$1"
    local prefix
    prefix="$2"
    local rel
    rel="$3"
    local items=()
    local total=0
    local entry name full_path child_rel

    if [ -n "$rel" ]; then
        ignore_push "$ROOT_DIR" "$rel"
    fi

    for entry in "$dir"/* "$dir"/.[!.]* "$dir"/..?*; do
        [ -e "$entry" ] || continue
        name="$(basename "$entry")"

        if [ "$name" = ".gitignore" ]; then
            continue
        fi

        child_rel="${rel:+$rel/}$name"

        if [ -d "$entry" ] && [ ! -L "$entry" ]; then
            if is_ignored "$child_rel" 1; then
                continue
            fi
        else
            if is_ignored "$child_rel" 0; then
                continue
            fi
        fi

        items+=("$name")
        total=$((total + 1))
    done

    if [ $total -eq 0 ]; then
        echo "${prefix}(empty directory)"
    else
        local sorted_items
        IFS=$'\n' sorted_items=($(sort <<<"${items[*]}"))
        unset IFS

        local i=0 item is_last line_prefix
        for item in "${sorted_items[@]}"; do
            i=$((i + 1))
            full_path="$dir/$item"
            child_rel="${rel:+$rel/}$item"
            is_last=0
            [ $i -eq $total ] && is_last=1

            if [ $is_last -eq 1 ]; then
                line_prefix="${prefix}└── "
            else
                line_prefix="${prefix}├── "
            fi

            if [ -d "$full_path" ] && [ ! -L "$full_path" ]; then
                echo "${line_prefix}$item/"
                if [ $is_last -eq 1 ]; then
                    list_dir "$full_path" "${prefix}    " "$child_rel"
                else
                    list_dir "$full_path" "${prefix}│   " "$child_rel"
                fi
            else
                echo "${line_prefix}$item"
            fi
        done
    fi

    if [ -n "$rel" ]; then
        ignore_pop
    fi
}

echo "$ROOT_DIR/"
list_dir "$ROOT_DIR" "" ""
