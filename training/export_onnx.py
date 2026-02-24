"""Export trained statement chaining classifier to ONNX using optimum-cli."""

from __future__ import annotations

import argparse
import subprocess


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", default="training/statement_chain_model")
    parser.add_argument("--output", default="models/statement-chain")
    args = parser.parse_args()

    cmd = [
        "optimum-cli",
        "export",
        "onnx",
        "--model",
        args.model_dir,
        "--task",
        "text-classification",
        args.output,
    ]

    print("Running:", " ".join(cmd))
    subprocess.run(cmd, check=True)
    print("ONNX export complete ->", args.output)


if __name__ == "__main__":
    main()
