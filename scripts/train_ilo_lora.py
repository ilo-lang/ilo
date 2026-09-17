#!/usr/bin/env python3
"""train_ilo_lora.py — Phase 4: LoRA fine-tune of Qwen2.5-0.5B-Instruct on
the ilo corpus (bench/finetune/train.jsonl).

Goal (PLAN.md Phase 4): internalize the curated spec so generated programs
no longer need the 4.1k-token spec in context — delete the spec-loading
term for fine-tuned models.

Loss is masked to assistant tokens only. CPU-friendly settings:
rank-16 LoRA, bf32 matmuls where available, batch 1 + grad accumulation.
"""

import json
import sys
from pathlib import Path

import torch
from datasets import Dataset
from peft import LoraConfig, get_peft_model
from transformers import (AutoModelForCausalLM, AutoTokenizer,
                          TrainingArguments, Trainer)

ROOT = Path(__file__).resolve().parent.parent
TRAIN = ROOT / "bench/finetune/train.jsonl"
OUT = ROOT / "bench/finetune/adapter"
BASE = "Qwen/Qwen2.5-0.5B-Instruct"
MAX_LEN = 4096
SPEC_TAG = "<<ILO_SPEC_INTERNALIZED>>"



def parse_args():
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument("--epochs", type=int, default=3)
    ap.add_argument("--out", default=None, help="override adapter output dir")
    return ap.parse_args()

EPOCHS = parse_args().epochs
def main() -> int:
    rows = [json.loads(l) for l in TRAIN.read_text().splitlines() if l.strip()]
    print(f"train rows: {len(rows)}")

    tok = AutoTokenizer.from_pretrained(BASE)
    if tok.pad_token is None:
        tok.pad_token = tok.eos_token

    model = AutoModelForCausalLM.from_pretrained(
        BASE, torch_dtype=torch.float32, low_cpu_mem_usage=True)
    model.config.use_cache = False

    lcfg = LoraConfig(
        r=16, lora_alpha=32, lora_dropout=0.05, bias="none",
        task_type="CAUSAL_LM",
        target_modules=["q_proj", "k_proj", "v_proj", "o_proj",
                        "gate_proj", "up_proj", "down_proj"],
    )
    model = get_peft_model(model, lcfg)
    model.print_trainable_parameters()

    def encode(row):
        msgs = [
            {"role": "system", "content": SPEC_TAG},
            *row["messages"][:-1],
        ]
        prompt_text = tok.apply_chat_template(
            msgs, tokenize=False, add_generation_prompt=True)
        full_text = tok.apply_chat_template(
            msgs + [row["messages"][-1]], tokenize=False)
        prompt_ids = tok(prompt_text, add_special_tokens=False)["input_ids"]
        full_ids = tok(full_text, add_special_tokens=False)["input_ids"]
        labels = [-100] * len(prompt_ids) + full_ids[len(prompt_ids):]
        labels = labels[:MAX_LEN]
        ids = full_ids[:MAX_LEN]
        return {"input_ids": ids, "labels": labels,
                "attention_mask": [1] * len(ids)}

    ds = Dataset.from_list(rows).map(encode, remove_columns=[
        c for c in rows[0].keys()])

    def collate(batch):
        mx = max(len(b["input_ids"]) for b in batch)
        input_ids, labels, attn = [], [], []
        for b in batch:
            pad = mx - len(b["input_ids"])
            input_ids.append(b["input_ids"] + [tok.pad_token_id] * pad)
            labels.append(b["labels"] + [-100] * pad)
            attn.append(b["attention_mask"] + [0] * pad)
        return {"input_ids": torch.tensor(input_ids),
                "labels": torch.tensor(labels),
                "attention_mask": torch.tensor(attn)}

    targs = TrainingArguments(
        output_dir=str(OUT if parse_args().out is None
                       else ROOT / parse_args().out),
        gradient_accumulation_steps=8,
        num_train_epochs=EPOCHS,
        learning_rate=2e-4,
        logging_steps=5,
        save_strategy="epoch",
        save_total_limit=1,
        report_to=[],
        use_cpu=True,
        gradient_checkpointing=True,
        dataloader_num_workers=0,
        remove_unused_columns=False,
    )
    Trainer(model=model, args=targs, train_dataset=ds,
            data_collator=collate).train()

    model.save_pretrained(str(OUT))
    print(f"adapter saved: {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
