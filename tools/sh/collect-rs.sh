#!/bin/bash
# =============================================================================
# collect-rs.sh – Gather Rust sources, config and build files honouring
#                 nested .gitignore files
# =============================================================================
#
# USAGE:
#   ./collect-rs.sh <directory> [output_file]
#
# Recursively walks <directory>, honouring the root .gitignore and any nested
# .gitignore files encountered along the way. Collects every *.rs, *.toml,
# README.md and Makefile, skipping anything matched by the active rules.
# Writes a single bundle with a file index and fenced code blocks.
# Cargo.lock is intentionally not included.
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ignore-lib.sh
. "$SCRIPT_DIR/ignore-lib.sh"

ROOT="${1:-}"
if [ -z "$ROOT" ] || [ ! -d "$ROOT" ]; then
    echo "Usage: $0 <directory> [output_file]" >&2
    exit 1
fi
ROOT="$(cd "$ROOT" && pwd)"
OUT="${2:-rs-bundle.txt}"

ignore_load "$ROOT"

FILES=()

want_file() {
    local name="$1"
    case "$name" in
        *.rs|*.toml|README.md|Makefile) return 0 ;;
    esac
    return 1
}

walk() {
    local dir="$1"
    local rel="$2"
    local entry name child_rel full

    if [ -n "$rel" ]; then
        ignore_push "$ROOT" "$rel"
    fi

    local entries=()
    for entry in "$dir"/* "$dir"/.[!.]* "$dir"/..?*; do
        [ -e "$entry" ] || continue
        entries+=("$entry")
    done

    if [ ${#entries[@]} -gt 0 ]; then
        local sorted
        IFS=$'\n' sorted=($(sort <<<"${entries[*]}"))
        unset IFS
        entries=("${sorted[@]}")
    fi

    for entry in "${entries[@]}"; do
        name="$(basename "$entry")"
        child_rel="${rel:+$rel/}$name"

        if [ -d "$entry" ] && [ ! -L "$entry" ]; then
            if is_ignored "$child_rel" 1; then
                continue
            fi
            walk "$entry" "$child_rel"
        else
            if is_ignored "$child_rel" 0; then
                continue
            fi
            if want_file "$name"; then
                FILES+=("$entry")
            fi
        fi
    done

    if [ -n "$rel" ]; then
        ignore_pop
    fi
}

walk "$ROOT" ""

if [ ${#FILES[@]} -eq 0 ]; then
    echo "No source, config or build files found under $ROOT" >&2
    exit 0
fi

{
    echo "# Rust, config and build bundle"
    echo "# root: $ROOT"
    echo "# files: ${#FILES[@]}"
    echo "# generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo

    echo "## File index"
    for f in "${FILES[@]}"; do
        echo "- ${f#"$ROOT"/}"
    done
    echo

    for f in "${FILES[@]}"; do
        rel="${f#"$ROOT"/}"
        case "$rel" in
            *.rs)
                lang="rust"
                ;;
            *.toml)
                lang="toml"
                ;;
            README.md)
                lang="markdown"
                ;;
            Makefile)
                lang="makefile"
                ;;
            *)
                lang=""
                ;;
        esac
        echo "## $rel"
        if [ -n "$lang" ]; then
            echo "\`\`\`$lang"
        else
            echo '```'
        fi
        cat "$f"
        echo '```'
        echo
    done
} > "$OUT"

echo "Wrote ${#FILES[@]} files to $OUT ($(wc -c < "$OUT") bytes)" >&2
