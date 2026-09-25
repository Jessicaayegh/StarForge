#
# Fuzz Corpus Hygiene Tool (PowerShell version)
#
# Minimizes, deduplicates, and reports on fuzz corpora under fuzz/corpus.
# Reduces CI time and reveals interesting inputs among noise.
#
# Usage:
#   .\scripts\corpus-hygiene.ps1 [-DryRun] [-Dedupe] [-Stats] [-All]
#
# Parameters:
#   -DryRun    Show what would be deleted without making changes
#   -Dedupe    Remove duplicate entries
#   -Minimize  Use cargo-fuzz minimize (slow, requires libfuzzer)
#   -Stats     Print corpus statistics
#   -All       Run all checks (dedupe + stats)

param(
    [switch]$DryRun = $false,
    [switch]$Dedupe = $false,
    [switch]$Minimize = $false,
    [switch]$Stats = $false,
    [switch]$All = $false
)

$ErrorActionPreference = "Stop"

# ── Helpers ────────────────────────────────────────────────────────────────

function Write-Info {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Green
}

function Write-Warn {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-Error-Custom {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
}

# ── Deduplication ─────────────────────────────────────────────────────────

function Deduplicate-Corpus {
    param([string]$Target)
    
    if (-not (Test-Path $Target -PathType Container)) {
        Write-Warn "Corpus directory not found: $Target"
        return
    }
    
    Write-Info "Deduplicating corpus: $Target"
    
    $seen_hashes = @{}
    $duplicates = @()
    $total = 0
    $dupe_count = 0
    
    # Get all files and compute hashes
    $files = Get-ChildItem -Path $Target -File -Recurse
    
    foreach ($file in $files) {
        $total++
        
        # Compute SHA256
        $hash = (Get-FileHash -Path $file.FullName -Algorithm SHA256).Hash
        
        if ($seen_hashes.ContainsKey($hash)) {
            $duplicates += $file.FullName
            $dupe_count++
            Write-Info "  Duplicate: $($file.Name) (hash: $($hash.Substring(0, 8))...)"
        } else {
            $seen_hashes[$hash] = $file.FullName
        }
    }
    
    Write-Info "  Total files: $total"
    Write-Info "  Duplicates found: $dupe_count"
    
    # Delete duplicates if not dry-run
    if ($dupe_count -gt 0) {
        if ($DryRun) {
            Write-Info "  [DRY-RUN] Would delete $dupe_count duplicate entries"
        } else {
            foreach ($file in $duplicates) {
                Remove-Item -Path $file -Force
                Write-Info "  Deleted: $file"
            }
            Write-Info "  Deleted $dupe_count duplicate entries"
        }
    }
}

# ── Corpus Statistics ──────────────────────────────────────────────────────

function Get-CorpusStats {
    param([string]$Target)
    
    if (-not (Test-Path $Target -PathType Container)) {
        Write-Warn "Corpus directory not found: $Target"
        return
    }
    
    $files = Get-ChildItem -Path $Target -File -Recurse
    $count = $files.Count
    
    if ($count -eq 0) {
        return
    }
    
    $total_size = ($files | Measure-Object -Property Length -Sum).Sum
    $sizes = $files | ForEach-Object { $_.Length } | Sort-Object
    $min_size = $sizes[0]
    $max_size = $sizes[-1]
    $avg_size = [math]::Floor($total_size / $count)
    
    Write-Host "  Count:     $count"
    Write-Host "  Total:     $total_size bytes"
    Write-Host "  Average:   $avg_size bytes"
    Write-Host "  Min:       $min_size bytes"
    Write-Host "  Max:       $max_size bytes"
}

# ── Main ───────────────────────────────────────────────────────────────────

function Main {
    # Default to --all if no options specified
    if (-not $Dedupe -and -not $Minimize -and -not $Stats -and -not $All) {
        $All = $true
    }
    
    if ($All) {
        $Dedupe = $true
        $Stats = $true
    }
    
    $ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
    $ProjectRoot = Split-Path -Parent $ScriptDir
    $CorpusDir = Join-Path $ProjectRoot "fuzz" "corpus"
    
    if (-not (Test-Path $CorpusDir -PathType Container)) {
        Write-Error-Custom "Corpus directory not found: $CorpusDir"
        exit 1
    }
    
    if ($DryRun) {
        Write-Info "Running in DRY-RUN mode (no changes will be made)"
    }
    
    Write-Host ""
    Write-Info "Fuzz Corpus Hygiene Report"
    Write-Host ""
    
    # Process each corpus
    $corpus_dirs = Get-ChildItem -Path $CorpusDir -Directory | Sort-Object Name
    
    foreach ($corpus in $corpus_dirs) {
        Write-Host "📦 Corpus: $($corpus.Name)"
        
        if ($Dedupe) {
            Deduplicate-Corpus -Target $corpus.FullName
        }
        
        if ($Stats) {
            Get-CorpusStats -Target $corpus.FullName
        }
        
        Write-Host ""
    }
    
    Write-Info "Corpus hygiene check complete"
}

Main
