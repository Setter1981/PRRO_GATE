import json
import os

transcript_path = "/mnt/c/Users/Користувач/.gemini/antigravity/brain/de409f46-3e36-4403-95eb-de9a6462bfdb/.system_generated/logs/transcript.jsonl"
output_path = "/mnt/d/PRRO_GATE/scratch/findings.txt"

files_of_interest = {
    "ingress_inbox.rs",
    "fiscal_documents.rs",
    "boot_phase.rs",
    "stage_acquire.rs",
    "stage_finalize.rs",
    "write_path_stage1_acquire.rs",
    "write_path_stage3_sign.rs",
    "finalize_helpers.rs",
    "write_path_stage5_finalize.rs"
}

with open(transcript_path, 'r', encoding='utf-8') as f_in, open(output_path, 'w', encoding='utf-8') as f_out:
    for line in f_in:
        try:
            step = json.loads(line)
            if step.get("source") == "MODEL" and "tool_calls" in step:
                for tc in step["tool_calls"]:
                    name = tc.get("name")
                    if name in ("replace_file_content", "multi_replace_file_content", "write_to_file"):
                        args = tc.get("args", {})
                        target_file = args.get("TargetFile", "")
                        
                        # Check if any file of interest is in the target_file path
                        matched = any(f_name in target_file for f_name in files_of_interest)
                        if matched:
                            f_out.write(f"=== STEP: {step.get('step_index')} ===\n")
                            f_out.write(f"TOOL: {name}\n")
                            f_out.write(f"FILE: {target_file}\n")
                            f_out.write(f"ARGS: {json.dumps(args, indent=2, ensure_ascii=False)}\n")
                            f_out.write("="*40 + "\n\n")
        except Exception as e:
            continue

print("Done parsing.")
