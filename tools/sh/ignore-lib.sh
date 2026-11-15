#!/bin/bash
# =============================================================================
# ignore-lib.sh – .gitignore support with nested rules for tree.sh and
#                 collect-rs.sh
# =============================================================================
#
# Public API:
#   ignore_load <root>
#       Load <root>/.gitignore into the base rule set.
#   is_ignored <rel_path> <is_dir>
#       Return 0 if <rel_path> is ignored by any active rule, 1 otherwise.
#
# Nested API:
#   ignore_push <root> <rel_dir>
#       Load <root>/<rel_dir>/.gitignore (if present) as a nested rule set.
#       Nested rules are matched against paths relative to <rel_dir>.
#   ignore_pop
#       Drop the most recent nested rule set.
#
# Semantics follow .gitignore closely enough for our purposes: blank lines and
# lines starting with # are skipped, patterns are matched against paths relative
# to the directory that owns the .gitignore, a trailing / means directory-only,
# and a leading ! negates. There is no ** support; patterns are matched either
# as a plain component or as a path suffix.
# =============================================================================

IGNORE_BASE_PATTERNS=()
IGNORE_BASE_NEGATIONS=()
IGNORE_BASE_DIRONLY=()

IGNORE_STACK_PATTERNS=()
IGNORE_STACK_NEGATIONS=()
IGNORE_STACK_DIRONLY=()
IGNORE_STACK_PREFIX=()

ignore_reset() {
    IGNORE_BASE_PATTERNS=()
    IGNORE_BASE_NEGATIONS=()
    IGNORE_BASE_DIRONLY=()
    IGNORE_STACK_PATTERNS=()
    IGNORE_STACK_NEGATIONS=()
    IGNORE_STACK_DIRONLY=()
    IGNORE_STACK_PREFIX=()
}

# ignore_parse_file <file>
#   Reads <file> and sets the three output arrays:
#     _PARSED_PATTERNS, _PARSED_NEGATIONS, _PARSED_DIRONLY
#   Each entry is one rule. Negations and dironly are "0" or "1".
ignore_parse_file() {
    local file="$1"
    _PARSED_PATTERNS=()
    _PARSED_NEGATIONS=()
    _PARSED_DIRONLY=()
    [ -f "$file" ] || return 0

    local line neg dirflag pat
    while IFS= read -r line || [ -n "$line" ]; do
        line="${line%$'\r'}"
        [ -z "$line" ] && continue
        case "$line" in
            \#*) continue ;;
        esac
        neg=0
        if [ "${line#!}" != "$line" ]; then
            neg=1
            line="${line#!}"
        fi
        dirflag=0
        case "$line" in
            */) dirflag=1; line="${line%/}" ;;
        esac
        [ -z "$line" ] && continue
        _PARSED_PATTERNS+=("$line")
        _PARSED_NEGATIONS+=("$neg")
        _PARSED_DIRONLY+=("$dirflag")
    done < "$file"
}

# ignore_load <root>
ignore_load() {
    local root="$1"
    ignore_parse_file "$root/.gitignore"
    IGNORE_BASE_PATTERNS=("${_PARSED_PATTERNS[@]}")
    IGNORE_BASE_NEGATIONS=("${_PARSED_NEGATIONS[@]}")
    IGNORE_BASE_DIRONLY=("${_PARSED_DIRONLY[@]}")
    IGNORE_STACK_PATTERNS=()
    IGNORE_STACK_NEGATIONS=()
    IGNORE_STACK_DIRONLY=()
    IGNORE_STACK_PREFIX=()
}

# ignore_push <root> <rel_dir>
ignore_push() {
    local root="$1"
    local rel_dir="$2"
    local file="$root/$rel_dir/.gitignore"
    ignore_parse_file "$file"
    IGNORE_STACK_PREFIX+=("$rel_dir")
    IGNORE_STACK_PATTERNS+=("$(printf '%s\n' "${_PARSED_PATTERNS[@]}")")
    IGNORE_STACK_NEGATIONS+=("$(printf '%s\n' "${_PARSED_NEGATIONS[@]}")")
    IGNORE_STACK_DIRONLY+=("$(printf '%s\n' "${_PARSED_DIRONLY[@]}")")
}

# ignore_pop
ignore_pop() {
    local n=${#IGNORE_STACK_PREFIX[@]}
    [ "$n" -eq 0 ] && return
    unset 'IGNORE_STACK_PREFIX[n-1]'
    unset 'IGNORE_STACK_PATTERNS[n-1]'
    unset 'IGNORE_STACK_NEGATIONS[n-1]'
    unset 'IGNORE_STACK_DIRONLY[n-1]'
}

# match_one <pattern> <rel_path> <is_dir>
match_one() {
    local pat="$1"
    local rel="$2"
    local is_dir="$3"
    case "$pat" in
        */)
            pat="${pat%/}"
            [ "$is_dir" -eq 1 ] || return 1
            ;;
    esac
    case "$pat" in
        */*)
            case "$rel" in
                $pat|$pat/*) return 0 ;;
            esac
            return 1
            ;;
        *)
            local comp
            IFS='/' read -r -a _parts <<< "$rel"
            for comp in "${_parts[@]}"; do
                case "$comp" in
                    $pat) return 0 ;;
                esac
            done
            return 1
            ;;
    esac
}

# ignore_apply <patterns> <negations> <dironly> <rel_path> <is_dir>
ignore_apply() {
    local patterns="$1"
    local negations="$2"
    local dironly="$3"
    local rel="$4"
    local is_dir="$5"
    [ -z "$patterns" ] && return 1
    local _p=() _n=() _d=() i
    IFS=$'\n' read -r -a _p <<< "$patterns"
    IFS=$'\n' read -r -a _n <<< "$negations"
    IFS=$'\n' read -r -a _d <<< "$dironly"
    local result=1
    for i in "${!_p[@]}"; do
        local pat="${_p[$i]}"
        [ -z "$pat" ] && continue
        local dirflag="${_d[$i]:-0}"
        if [ "$dirflag" = "1" ] && [ "$is_dir" -ne 1 ]; then
            continue
        fi
        if match_one "$pat" "$rel" "$is_dir"; then
            if [ "${_n[$i]:-0}" = "1" ]; then
                result=1
            else
                result=0
            fi
        fi
    done
    return $result
}

# is_ignored <rel_path> <is_dir>
is_ignored() {
    local rel="$1"
    local is_dir="$2"
    local result=1

    if ignore_apply "$(printf '%s\n' "${IGNORE_BASE_PATTERNS[@]}")" \
                    "$(printf '%s\n' "${IGNORE_BASE_NEGATIONS[@]}")" \
                    "$(printf '%s\n' "${IGNORE_BASE_DIRONLY[@]}")" \
                    "$rel" "$is_dir"; then
        result=0
    fi

    local i n=${#IGNORE_STACK_PREFIX[@]}
    for ((i=0; i<n; i++)); do
        local prefix="${IGNORE_STACK_PREFIX[$i]}"
        local sub="$rel"
        case "$rel" in
            "$prefix"/*) sub="${rel#"$prefix"/}" ;;
            *) continue ;;
        esac
        if ignore_apply "${IGNORE_STACK_PATTERNS[$i]}" \
                        "${IGNORE_STACK_NEGATIONS[$i]}" \
                        "${IGNORE_STACK_DIRONLY[$i]}" \
                        "$sub" "$is_dir"; then
            result=0
        fi
    done

    return $result
}
