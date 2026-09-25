#!/usr/bin/env bash
#
# Fuzz Corpus Hygiene Tool
#
# Minimizes, deduplicates, and reports on fuzz corpora under fuzz/corpus.
# Reduces CI time and reveals interesting inputs among noise.
#
# Usage:
#   ./scripts/corpus-hygiene.sh [OPTIONS]
#
# Options:
#   --dry-run          Show what would be deleted without making changes
#   --dedupe           Remove duplicate entries
#   --minimize         Use cargo-fuzz minimize (slow, requires libfuzzer)
#   --stats            Print corpus statistics
#   --all              Run all checks (dedupe + stats)
#   --help             Show this message

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CORPUS_DIR="$PROJECT_ROOT/fuzz/corpus"

DRY_RUN=false
DEDUPE=false
MINIMIZE=false
STATS=false
ALL=false

# ── Helpers ────────────────────────────────────────────────────────────────

log_info() {
    echo "[INFO] $*" >&2
}

log_warn() {
    echo "[WARN] $*" >&2
}

log_error() {
    echo "[ERROR] $*" >&2
}

show_help() {
    head -30 "$0" | tail -25
}

# ── Deduplication ─────────────────────────────────────────────────────────

deduplicate_corpus() {
    local target="$1"
    
    if [ ! -d "$target" ]; then
        log_warn "Corpus directory not found: $target"
        return 1
    fi
    
    log_info "Deduplicating corpus: $target"
    
    local -A seen_hashes
    local -A duplicates
    local total=0
    local dupe_count=0
    
    while IFS= read -r -d '' file; do
        total=$((total + 1))
        
        # Compute SHA256 of file content
        local hash
        if command -v sha256sum &> /dev/null; then
            hash=$(sha256sum "$file" | cut -d' ' -f1)
        elif command -v shasum &> /dev/null; then
            hash=$(shasum -a 256 "$file" | cut -d' ' -f1)
        else
            log_error "Neither sha256sum nor shasum found"
            return 1
        fi
        
        # Track duplicates
        if [[ -v "seen_hashes[$hash]" ]]; then
            duplicates["$file"]=1
            dupe_count=$((dupe_count + 1))
            log_info "  Duplicate: $file (hash: ${hash:0:8}...)"
        else
            seen_hashes["$hash"]=1
        fi
    done < <(find "$target" -type f -print0)
    
    log_info "  Total files: $total"
    log_info "  Duplicates found: $dupe_count"
    
    # Delete duplicates if not dry-run
    if [[ $dupe_count -gt 0 ]]; then
        if [[ $DRY_RUN == true ]]; then
            log_info "  [DRY-RUN] Would delete $dupe_count duplicate entries"
        else
            for file in "${!duplicates[@]}"; do
                rm -f "$file"
                log_info "  Deleted: $file"
            done
            log_info "  Deleted $dupe_count duplicate entries"
        fi
    fi
}

# ── Corpus Statistics ──────────────────────────────────────────────────────

corpus_stats() {
    local target="$1"
    
    if [ ! -d "$target" ]; then
        log_warn "Corpus directory not found: $target"
        return 1
    fi
    
    local count=0
    local total_size=0
    local min_size=999999999
    local max_size=0
    
    while IFS= read -r -d '' file; do
        count=$((count + 1))
        local size
        size=$(stat -f%z "$file" 2>/dev/null || stat -c%s "$file" 2>/dev/null || echo 0)
        total_size=$((total_size + size))
        
        if [[ $size -lt $min_size ]]; then
            min_size=$size
        fi
        if [[ $size -gt $max_size ]]; then
            max_size=$size
        fi
    done < <(find "$target" -type f -print0)
    
    if [[ $count -eq 0 ]]; then
        return 0
    fi
    
    local avg_size=$((total_size / count))
    
    echo "  Count:     $count"
    echo "  Total:     $total_size bytes"
    echo "  Average:   $avg_size bytes"
    echo "  Min:       $min_size bytes"
    echo "  Max:       $max_size bytes"
}

# ── Main ───────────────────────────────────────────────────────────────────

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --dry-run)
                DRY_RUN=true
                shift
                ;;
            --dedupe)
                DEDUPE=true
                shift
                ;;
            --minimize)
                MINIMIZE=true
                shift
                ;;
            --stats)
                STATS=true
                shift
                ;;
            --all)
                ALL=true
                DEDUPE=true
                STATS=true
                shift
                ;;
            --help)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done
    
    # Default to --all if no options specified
    if ! $DEDUPE && ! $MINIMIZE && ! $STATS && ! $ALL; then
        ALL=true
        DEDUPE=true
        STATS=true
    fi
}

main() {
    parse_args "$@"
    
    if [ ! -d "$CORPUS_DIR" ]; then
        log_error "Corpus directory not found: $CORPUS_DIR"
        exit 1
    fi
    
    if [[ $DRY_RUN == true ]]; then
        log_info "Running in DRY-RUN mode (no changes will be made)"
    fi
    
    echo ""
    log_info "Fuzz Corpus Hygiene Report"
    echo ""
    
    # Process each corpus
    local corpus_dirs
    corpus_dirs=$(find "$CORPUS_DIR" -maxdepth 1 -type d | sort)
    
    for corpus_path in $corpus_dirs; do
        if [ "$corpus_path" = "$CORPUS_DIR" ]; then
            continue
        fi
        
        local corpus_name
        corpus_name=$(basename "$corpus_path")
        
        echo "📦 Corpus: $corpus_name"
        
        if [[ $DEDUPE == true ]]; then
            deduplicate_corpus "$corpus_path" || true
        fi
        
        if [[ $STATS == true ]]; then
            corpus_stats "$corpus_path"
        fi
        
        echo ""
    done
    
    log_info "Corpus hygiene check complete"
}

main "$@"
