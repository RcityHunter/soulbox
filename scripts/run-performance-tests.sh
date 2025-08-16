#!/bin/bash

# SoulBox Performance Regression Test Suite
# This script runs comprehensive performance tests and generates reports

set -e

# Configuration
BASELINE_BRANCH="${BASELINE_BRANCH:-main}"
TEST_DURATION="${TEST_DURATION:-60}"
WARMUP_DURATION="${WARMUP_DURATION:-10}"
REPORT_DIR="target/criterion"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
REPORT_FILE="performance_report_${TIMESTAMP}.html"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== SoulBox Performance Regression Test Suite ===${NC}"
echo -e "Timestamp: $(date)"
echo -e "Baseline Branch: ${BASELINE_BRANCH}"
echo -e "Test Duration: ${TEST_DURATION}s"
echo -e "Report Directory: ${REPORT_DIR}"
echo ""

# Check prerequisites
echo -e "${YELLOW}Checking prerequisites...${NC}"

if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: cargo not found. Please install Rust.${NC}"
    exit 1
fi

if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: docker not found. Please install Docker.${NC}"
    exit 1
fi

if ! docker info &> /dev/null; then
    echo -e "${RED}Error: Docker daemon not running. Please start Docker.${NC}"
    exit 1
fi

echo -e "${GREEN}Prerequisites check passed.${NC}"

# Build project in release mode for accurate performance testing
echo -e "${YELLOW}Building SoulBox in release mode...${NC}"
cargo build --release --quiet

# Create report directory
mkdir -p "${REPORT_DIR}"

# Function to run benchmarks with error handling
run_benchmark() {
    local benchmark_name="$1"
    local args="$2"
    
    echo -e "${YELLOW}Running ${benchmark_name} benchmark...${NC}"
    
    if cargo bench --bench regression_tests -- $args 2>&1 | tee "${REPORT_DIR}/${benchmark_name}_${TIMESTAMP}.log"; then
        echo -e "${GREEN}✓ ${benchmark_name} benchmark completed successfully${NC}"
        return 0
    else
        echo -e "${RED}✗ ${benchmark_name} benchmark failed${NC}"
        return 1
    fi
}

# Function to check system resources
check_system_resources() {
    echo -e "${YELLOW}Checking system resources...${NC}"
    
    # Check available memory
    AVAILABLE_MEMORY=$(free -m | awk 'NR==2{printf "%.1f", $7/1024}')
    if (( $(echo "$AVAILABLE_MEMORY < 2.0" | bc -l) )); then
        echo -e "${YELLOW}Warning: Low available memory (${AVAILABLE_MEMORY}GB). Results may be affected.${NC}"
    fi
    
    # Check CPU load
    CPU_LOAD=$(uptime | awk -F'load average:' '{ print $2 }' | awk '{ print $1 }' | sed 's/,//')
    if (( $(echo "$CPU_LOAD > 2.0" | bc -l) )); then
        echo -e "${YELLOW}Warning: High CPU load (${CPU_LOAD}). Results may be affected.${NC}"
    fi
    
    # Check disk space
    DISK_USAGE=$(df -h . | awk 'NR==2 {print $5}' | sed 's/%//')
    if [ "$DISK_USAGE" -gt 90 ]; then
        echo -e "${YELLOW}Warning: High disk usage (${DISK_USAGE}%). Results may be affected.${NC}"
    fi
    
    echo -e "${GREEN}System resources check completed.${NC}"
}

# Function to generate performance baseline
generate_baseline() {
    echo -e "${YELLOW}Generating performance baseline...${NC}"
    
    # Check if baseline exists
    if [ -f "${REPORT_DIR}/baseline.json" ]; then
        echo -e "${GREEN}Using existing baseline from ${REPORT_DIR}/baseline.json${NC}"
    else
        echo -e "${YELLOW}No baseline found. Creating new baseline...${NC}"
        
        # Run quick baseline benchmarks
        cargo bench --bench regression_tests -- --save-baseline baseline
        
        echo -e "${GREEN}Baseline created and saved.${NC}"
    fi
}

# Function to compare with baseline
compare_with_baseline() {
    echo -e "${YELLOW}Comparing current results with baseline...${NC}"
    
    if [ -f "${REPORT_DIR}/baseline.json" ]; then
        cargo bench --bench regression_tests -- --load-baseline baseline
    else
        echo -e "${YELLOW}No baseline available for comparison.${NC}"
    fi
}

# Function to generate HTML report
generate_html_report() {
    echo -e "${YELLOW}Generating HTML performance report...${NC}"
    
    cat > "${REPORT_DIR}/${REPORT_FILE}" << EOF
<!DOCTYPE html>
<html>
<head>
    <title>SoulBox Performance Report - ${TIMESTAMP}</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 20px; }
        .header { background-color: #f0f0f0; padding: 20px; border-radius: 5px; }
        .section { margin: 20px 0; padding: 15px; border: 1px solid #ddd; border-radius: 5px; }
        .success { color: green; }
        .warning { color: orange; }
        .error { color: red; }
        .metric { display: flex; justify-content: space-between; margin: 5px 0; }
        .metric-name { font-weight: bold; }
        .metric-value { font-family: monospace; }
        table { width: 100%; border-collapse: collapse; margin: 10px 0; }
        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
        th { background-color: #f2f2f2; }
    </style>
</head>
<body>
    <div class="header">
        <h1>SoulBox Performance Report</h1>
        <p><strong>Generated:</strong> $(date)</p>
        <p><strong>Baseline Branch:</strong> ${BASELINE_BRANCH}</p>
        <p><strong>Test Duration:</strong> ${TEST_DURATION}s</p>
    </div>

    <div class="section">
        <h2>System Information</h2>
        <div class="metric">
            <span class="metric-name">OS:</span>
            <span class="metric-value">$(uname -s) $(uname -r)</span>
        </div>
        <div class="metric">
            <span class="metric-name">CPU:</span>
            <span class="metric-value">$(nproc) cores</span>
        </div>
        <div class="metric">
            <span class="metric-name">Memory:</span>
            <span class="metric-value">$(free -h | awk 'NR==2{print $2}')</span>
        </div>
        <div class="metric">
            <span class="metric-name">Rust Version:</span>
            <span class="metric-value">$(rustc --version)</span>
        </div>
    </div>

    <div class="section">
        <h2>Performance Benchmarks</h2>
        <p>Detailed benchmark results are available in the Criterion reports:</p>
        <ul>
            <li><a href="container_startup/report/index.html">Container Startup Performance</a></li>
            <li><a href="code_execution/report/index.html">Code Execution Performance</a></li>
            <li><a href="snapshot_operations/report/index.html">Snapshot Operations Performance</a></li>
            <li><a href="cpu_optimization/report/index.html">CPU Optimization Performance</a></li>
            <li><a href="memory_performance/report/index.html">Memory Performance</a></li>
            <li><a href="concurrency_performance/report/index.html">Concurrency Performance</a></li>
            <li><a href="e2e_workflow/report/index.html">End-to-End Workflow Performance</a></li>
        </ul>
    </div>

    <div class="section">
        <h2>Performance Regression Checks</h2>
        <table>
            <thead>
                <tr>
                    <th>Metric</th>
                    <th>Baseline</th>
                    <th>Current</th>
                    <th>Change</th>
                    <th>Status</th>
                </tr>
            </thead>
            <tbody>
                <tr>
                    <td>Container Startup Time</td>
                    <td>500ms</td>
                    <td id="container-startup">-</td>
                    <td id="container-startup-change">-</td>
                    <td id="container-startup-status">PENDING</td>
                </tr>
                <tr>
                    <td>Code Execution Latency</td>
                    <td>100ms</td>
                    <td id="execution-latency">-</td>
                    <td id="execution-latency-change">-</td>
                    <td id="execution-latency-status">PENDING</td>
                </tr>
                <tr>
                    <td>Snapshot Creation Time</td>
                    <td>2s</td>
                    <td id="snapshot-creation">-</td>
                    <td id="snapshot-creation-change">-</td>
                    <td id="snapshot-creation-status">PENDING</td>
                </tr>
                <tr>
                    <td>CPU Utilization</td>
                    <td>&lt;40%</td>
                    <td id="cpu-utilization">-</td>
                    <td id="cpu-utilization-change">-</td>
                    <td id="cpu-utilization-status">PENDING</td>
                </tr>
                <tr>
                    <td>Memory Usage</td>
                    <td>&lt;256MB</td>
                    <td id="memory-usage">-</td>
                    <td id="memory-usage-change">-</td>
                    <td id="memory-usage-status">PENDING</td>
                </tr>
            </tbody>
        </table>
    </div>

    <div class="section">
        <h2>Test Logs</h2>
        <p>Detailed test logs are available in the following files:</p>
        <ul>
$(find "${REPORT_DIR}" -name "*_${TIMESTAMP}.log" | sed 's|^.*/||' | sed 's/^/            <li><a href="/' | sed 's/$/">&<\/a><\/li>/')
        </ul>
    </div>

    <div class="section">
        <h2>Recommendations</h2>
        <ul>
            <li>Monitor container startup times - target &lt;500ms</li>
            <li>Keep CPU utilization below 40% under normal load</li>
            <li>Ensure snapshot operations complete within 2 seconds</li>
            <li>Memory usage should not exceed 256MB per sandbox</li>
            <li>Run these tests before each release to catch performance regressions</li>
        </ul>
    </div>
</body>
</html>
EOF

    echo -e "${GREEN}HTML report generated: ${REPORT_DIR}/${REPORT_FILE}${NC}"
}

# Main execution flow
main() {
    check_system_resources
    
    echo -e "${YELLOW}Starting performance test suite...${NC}"
    
    # Generate or load baseline
    generate_baseline
    
    # Run individual benchmark suites
    local failed_tests=0
    
    if ! run_benchmark "container_startup" "container_startup"; then
        ((failed_tests++))
    fi
    
    if ! run_benchmark "code_execution" "code_execution"; then
        ((failed_tests++))
    fi
    
    if ! run_benchmark "snapshot_operations" "snapshot_operations"; then
        ((failed_tests++))
    fi
    
    if ! run_benchmark "cpu_optimization" "cpu_optimization"; then
        ((failed_tests++))
    fi
    
    if ! run_benchmark "memory_performance" "memory_performance"; then
        ((failed_tests++))
    fi
    
    if ! run_benchmark "concurrency_performance" "concurrency_performance"; then
        ((failed_tests++))
    fi
    
    if ! run_benchmark "e2e_workflow" "e2e_workflow"; then
        ((failed_tests++))
    fi
    
    # Compare with baseline
    compare_with_baseline
    
    # Generate reports
    generate_html_report
    
    # Summary
    echo ""
    echo -e "${BLUE}=== Performance Test Summary ===${NC}"
    
    if [ $failed_tests -eq 0 ]; then
        echo -e "${GREEN}✓ All performance tests passed successfully!${NC}"
        echo -e "${GREEN}✓ No performance regressions detected.${NC}"
    else
        echo -e "${RED}✗ $failed_tests performance test(s) failed.${NC}"
        echo -e "${RED}✗ Performance regressions may be present.${NC}"
    fi
    
    echo -e "📊 Detailed reports available in: ${REPORT_DIR}"
    echo -e "📋 HTML report: ${REPORT_DIR}/${REPORT_FILE}"
    echo -e "📈 Criterion reports: ${REPORT_DIR}/*/report/index.html"
    
    # Exit with error if tests failed
    exit $failed_tests
}

# Handle script interruption
trap 'echo -e "\n${YELLOW}Performance tests interrupted. Cleaning up...${NC}"; exit 1' INT TERM

# Run main function
main "$@"