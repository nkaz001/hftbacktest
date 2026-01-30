"""
Backtest Alignment Tools

Tools for verifying alignment between Python and Rust strategy implementations.
"""

from .compare import (
    load_rust_results,
    load_python_results,
    compute_alignment_metrics,
    plot_comparison,
    AlignmentMetrics,
)

__all__ = [
    'load_rust_results',
    'load_python_results',
    'compute_alignment_metrics',
    'plot_comparison',
    'AlignmentMetrics',
]
