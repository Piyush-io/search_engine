#!/usr/bin/env python3
"""
Variant Scoring Logic for Niche Tuning Loop

Calculates weighted scores prioritizing exact/acronym gains while penalizing conceptual regressions.

Usage:
    python3 scripts/score_variant.py <eval_results.json> <baseline_metrics.json> [--output <output.json>]

Example:
    python3 scripts/score_variant.py reports/niche_tuning/variant_x.json reports/niche_tuning/baseline_metrics.json
"""

import json
import sys
import argparse
from typing import Dict, Any, Optional


def extract_bucket_metric(bucket_metrics: list, bucket_name: str, metric: str = "mrr") -> float:
    """Extract a specific metric from bucket metrics."""
    for bucket in bucket_metrics:
        if bucket.get("bucket", "").lower() == bucket_name.lower():
            return float(bucket.get(metric, 0.0))
    return 0.0


def compute_weighted_score(metrics: Dict[str, Any]) -> Dict[str, Any]:
    """
    Compute weighted score using the formula:
    score = 0.4*MRR + 0.2*Recall@10 + 0.2*ExactMRR + 0.2*AcronymMRR - regressions*0.5
    
    Prioritizes exact/acronym gains, penalizes conceptual paraphrase regressions.
    """
    # Extract primary metrics
    mrr = metrics.get("mrr", 0.0)
    
    # Extract recall@10
    recall_at = metrics.get("recall_at", [])
    recall_10 = 0.0
    for k, v in recall_at:
        if k == 10:
            recall_10 = v
            break
    
    # If recall_at is list of lists in JSON
    if not recall_10 and recall_at:
        for item in recall_at:
            if isinstance(item, list) and len(item) >= 2 and item[0] == 10:
                recall_10 = item[1]
                break
    
    # Extract bucket metrics
    bucket_metrics = metrics.get("bucket_metrics", [])
    
    exact_mrr = extract_bucket_metric(bucket_metrics, "exactidentifier")
    acronym_mrr = extract_bucket_metric(bucket_metrics, "acronymdisambiguation")
    conceptual_mrr = extract_bucket_metric(bucket_metrics, "conceptualparaphrase")
    
    # Compute base score
    base_score = (
        0.4 * mrr +
        0.2 * recall_10 +
        0.2 * exact_mrr +
        0.2 * acronym_mrr
    )
    
    # Penalize conceptual regressions
    # If conceptual MRR is below threshold, apply penalty
    conceptual_threshold = 0.6  # Expected minimum for conceptual queries
    conceptual_penalty = 0.0
    
    if conceptual_mrr < conceptual_threshold:
        regression_amount = conceptual_threshold - conceptual_mrr
        conceptual_penalty = regression_amount * 0.5
    
    # Bonus for strong exact/acronym performance
    bonus = 0.0
    if exact_mrr > 0.9:
        bonus += 0.05
    if acronym_mrr > 0.8:
        bonus += 0.03
    
    final_score = base_score - conceptual_penalty + bonus
    
    return {
        "base_score": round(base_score, 6),
        "conceptual_penalty": round(conceptual_penalty, 6),
        "exact_acronym_bonus": round(bonus, 6),
        "final_score": round(final_score, 6),
        "component_breakdown": {
            "mrr_contribution": round(0.4 * mrr, 6),
            "recall_10_contribution": round(0.2 * recall_10, 6),
            "exact_mrr_contribution": round(0.2 * exact_mrr, 6),
            "acronym_mrr_contribution": round(0.2 * acronym_mrr, 6)
        },
        "raw_metrics": {
            "mrr": mrr,
            "recall_at_10": recall_10,
            "exact_mrr": exact_mrr,
            "acronym_mrr": acronym_mrr,
            "conceptual_mrr": conceptual_mrr
        }
    }


def compute_relative_improvements(variant_metrics: Dict[str, Any], 
                                   baseline_metrics: Dict[str, Any]) -> Dict[str, Any]:
    """Compute relative improvements vs baseline."""
    improvements = {}
    
    # Primary metrics
    for metric in ["mrr", "num_queries"]:
        v_val = variant_metrics.get(metric, 0.0)
        b_val = baseline_metrics.get(metric, 0.0)
        if b_val > 0:
            rel_change = (v_val - b_val) / b_val
            improvements[metric] = {
                "absolute_change": round(v_val - b_val, 6),
                "relative_change": round(rel_change, 6),
                "percent_change": round(rel_change * 100, 2)
            }
        else:
            improvements[metric] = {
                "absolute_change": round(v_val - b_val, 6),
                "relative_change": None,
                "percent_change": None
            }
    
    # Recall@10
    def get_recall_10(m):
        recall_at = m.get("recall_at", [])
        for item in recall_at:
            if isinstance(item, list) and len(item) >= 2 and item[0] == 10:
                return item[1]
            if isinstance(item, tuple):
                if item[0] == 10:
                    return item[1]
        return 0.0
    
    v_recall = get_recall_10(variant_metrics)
    b_recall = get_recall_10(baseline_metrics)
    
    if b_recall > 0:
        rel_change = (v_recall - b_recall) / b_recall
        improvements["recall_at_10"] = {
            "absolute_change": round(v_recall - b_recall, 6),
            "relative_change": round(rel_change, 6),
            "percent_change": round(rel_change * 100, 2)
        }
    
    # Bucket metrics
    v_buckets = variant_metrics.get("bucket_metrics", [])
    b_buckets = baseline_metrics.get("bucket_metrics", [])
    
    bucket_names = ["exactidentifier", "acronymdisambiguation", "conceptualparaphrase"]
    
    for bucket_name in bucket_names:
        v_mrr = extract_bucket_metric(v_buckets, bucket_name)
        b_mrr = extract_bucket_metric(b_buckets, bucket_name)
        
        if b_mrr > 0:
            rel_change = (v_mrr - b_mrr) / b_mrr
            improvements[f"{bucket_name}_mrr"] = {
                "absolute_change": round(v_mrr - b_mrr, 6),
                "relative_change": round(rel_change, 6),
                "percent_change": round(rel_change * 100, 2)
            }
        else:
            improvements[f"{bucket_name}_mrr"] = {
                "absolute_change": round(v_mrr - b_mrr, 6),
                "relative_change": None,
                "percent_change": None
            }
    
    return improvements


def classify_result(improvements: Dict[str, Any]) -> str:
    """Classify the tuning result based on improvements."""
    mrr_change = improvements.get("mrr", {}).get("percent_change", 0)
    exact_change = improvements.get("exactidentifier_mrr", {}).get("percent_change", 0)
    acronym_change = improvements.get("acronymdisambiguation_mrr", {}).get("percent_change", 0)
    conceptual_change = improvements.get("conceptualparaphrase_mrr", {}).get("percent_change", 0)
    
    # Strong improvement
    if mrr_change > 5 and exact_change > 3 and conceptual_change > -3:
        return "strong_improvement"
    
    # Moderate improvement
    if mrr_change > 2 and conceptual_change > -5:
        return "moderate_improvement"
    
    # Mixed results
    if abs(mrr_change) < 2 and abs(conceptual_change) < 5:
        return "neutral"
    
    # Regression
    if mrr_change < -3 or conceptual_change < -8:
        return "regression"
    
    return "mixed"


def score_variant(variant_path: str, baseline_path: Optional[str] = None) -> Dict[str, Any]:
    """Score a single variant against baseline."""
    # Load variant metrics
    with open(variant_path, 'r') as f:
        variant_data = json.load(f)
    
    # Handle both direct metrics and wrapped report format
    if "metrics" in variant_data:
        variant_metrics = variant_data["metrics"]
    else:
        variant_metrics = variant_data
    
    # Compute weighted score
    score_result = compute_weighted_score(variant_metrics)
    
    result = {
        "variant_file": variant_path,
        "scoring": score_result,
        "classification": "unknown"
    }
    
    # Compute relative improvements if baseline provided
    if baseline_path:
        with open(baseline_path, 'r') as f:
            baseline_data = json.load(f)
        
        if "metrics" in baseline_data:
            baseline_metrics = baseline_data["metrics"]
        else:
            baseline_metrics = baseline_data
        
        improvements = compute_relative_improvements(variant_metrics, baseline_metrics)
        result["improvements_vs_baseline"] = improvements
        result["classification"] = classify_result(improvements)
    
    return result


def main():
    parser = argparse.ArgumentParser(
        description="Score a variant configuration against baseline"
    )
    parser.add_argument("variant", help="Path to variant eval_results.json")
    parser.add_argument("baseline", nargs="?", help="Path to baseline metrics.json")
    parser.add_argument("--output", "-o", help="Output file path (default: stdout)")
    parser.add_argument("--format", choices=["json", "summary"], default="json",
                        help="Output format")
    
    args = parser.parse_args()
    
    try:
        result = score_variant(args.variant, args.baseline)
        
        if args.format == "json":
            output = json.dumps(result, indent=2)
        else:
            # Summary format
            lines = [
                "=" * 50,
                "Variant Scoring Results",
                "=" * 50,
                f"Variant: {result['variant_file']}",
                f"Classification: {result['classification']}",
                "",
                "Weighted Score Breakdown:",
                f"  Base Score:      {result['scoring']['base_score']:.4f}",
                f"  Penalty:         {result['scoring']['conceptual_penalty']:.4f}",
                f"  Bonus:           {result['scoring']['exact_acronym_bonus']:.4f}",
                f"  Final Score:     {result['scoring']['final_score']:.4f}",
                "",
                "Raw Metrics:",
                f"  MRR:             {result['scoring']['raw_metrics']['mrr']:.4f}",
                f"  Recall@10:       {result['scoring']['raw_metrics']['recall_at_10']:.4f}",
                f"  Exact MRR:       {result['scoring']['raw_metrics']['exact_mrr']:.4f}",
                f"  Acronym MRR:     {result['scoring']['raw_metrics']['acronym_mrr']:.4f}",
                f"  Conceptual MRR:  {result['scoring']['raw_metrics']['conceptual_mrr']:.4f}",
            ]
            
            if "improvements_vs_baseline" in result:
                lines.extend([
                    "",
                    "Improvements vs Baseline:",
                ])
                for metric, data in result["improvements_vs_baseline"].items():
                    pct = data.get("percent_change")
                    pct_str = f"{pct:+.2f}%" if pct is not None else "N/A"
                    lines.append(f"  {metric}: {pct_str}")
            
            lines.append("=" * 50)
            output = "\n".join(lines)
        
        if args.output:
            with open(args.output, 'w') as f:
                f.write(output)
            print(f"Results written to: {args.output}", file=sys.stderr)
        else:
            print(output)
        
        return 0
        
    except FileNotFoundError as e:
        print(f"Error: File not found - {e}", file=sys.stderr)
        return 1
    except json.JSONDecodeError as e:
        print(f"Error: Invalid JSON - {e}", file=sys.stderr)
        return 1
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
