"""Train DistilBERT statement chaining classifier.

Input JSONL format (from `cargo run --bin label`):
{
  "current_sentence": "...",
  "prev_1": "...",
  "prev_2": "...",
  "prev_3": "...",
  "label": 0|1|2|3
}

We expand each row into candidate pairs:
  [current, prev_i] -> binary label (depends / not depends)
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import torch
from torch.utils.data import Dataset
from transformers import (
    AutoModelForSequenceClassification,
    AutoTokenizer,
    Trainer,
    TrainingArguments,
)


class PairDataset(Dataset):
    def __init__(self, rows, tokenizer, max_len=256):
        self.examples = []
        for row in rows:
            cur = row["current_sentence"].strip()
            label = int(row.get("label", 0))

            for i in (1, 2, 3):
                prev = (row.get(f"prev_{i}") or "").strip()
                if not prev:
                    continue

                y = 1 if label == i else 0
                enc = tokenizer(
                    cur,
                    prev,
                    truncation=True,
                    padding="max_length",
                    max_length=max_len,
                )
                enc["labels"] = y
                self.examples.append(enc)

    def __len__(self):
        return len(self.examples)

    def __getitem__(self, idx):
        ex = self.examples[idx]
        return {k: torch.tensor(v) for k, v in ex.items()}


def load_rows(path: Path):
    rows = []
    with path.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            rows.append(json.loads(line))
    return rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", default="training/labels.jsonl")
    parser.add_argument("--model", default="distilbert-base-uncased")
    parser.add_argument("--output", default="training/statement_chain_model")
    parser.add_argument("--epochs", type=int, default=2)
    parser.add_argument("--batch-size", type=int, default=16)
    args = parser.parse_args()

    rows = load_rows(Path(args.input))
    if not rows:
        raise SystemExit(f"No rows found in {args.input}")

    tokenizer = AutoTokenizer.from_pretrained(args.model)
    model = AutoModelForSequenceClassification.from_pretrained(args.model, num_labels=2)

    ds = PairDataset(rows, tokenizer)
    if len(ds) == 0:
        raise SystemExit("No candidate pairs were built from labels")

    train_args = TrainingArguments(
        output_dir=args.output,
        per_device_train_batch_size=args.batch_size,
        num_train_epochs=args.epochs,
        logging_steps=20,
        save_strategy="epoch",
        remove_unused_columns=False,
        report_to=[],
    )

    trainer = Trainer(model=model, args=train_args, train_dataset=ds)
    trainer.train()

    Path(args.output).mkdir(parents=True, exist_ok=True)
    trainer.save_model(args.output)
    tokenizer.save_pretrained(args.output)
    print(f"Saved model to {args.output}")


if __name__ == "__main__":
    main()
