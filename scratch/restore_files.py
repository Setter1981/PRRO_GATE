import json
import os

transcript_path = "/mnt/c/Users/Користувач/.gemini/antigravity/brain/de409f46-3e36-4403-95eb-de9a6462bfdb/.system_generated/logs/transcript.jsonl"

steps_to_apply = {
    # 1n9 Core Migration Steps
    162, 165, 175, 177, 185, 187, 273, 306, 316, 328, 351, 455
}

def apply_replace_file_content(args):
    target_file = args.get("TargetFile", "")
    # Normalize path to windows/wsl compatibility
    target_file = target_file.replace("d:\\PRRO_GATE\\", "/mnt/d/PRRO_GATE/")
    target_file = target_file.replace("d:\\\\PRRO_GATE\\\\", "/mnt/d/PRRO_GATE/")
    target_file = target_file.replace("\\\\", "/")
    target_file = target_file.replace("\\", "/")
    
    start_line = args.get("StartLine")
    end_line = args.get("EndLine")
    target_content = args.get("TargetContent", "")
    replacement_content = args.get("ReplacementContent", "")
    
    if not os.path.exists(target_file):
        print(f"Error: file not found {target_file}")
        return False
        
    with open(target_file, 'r', encoding='utf-8') as f:
        content = f.read()
        
    if target_content in content:
        new_content = content.replace(target_content, replacement_content, 1)
        with open(target_file, 'w', encoding='utf-8') as f_out:
            f_out.write(new_content)
        print(f"Successfully applied replacement to {target_file}")
        return True
    else:
        # Try raw unescaped search if literal match failed
        print(f"Warning: TargetContent not found in {target_file}")
        return False

def apply_multi_replace_file_content(args):
    target_file = args.get("TargetFile", "")
    target_file = target_file.replace("d:\\PRRO_GATE\\", "/mnt/d/PRRO_GATE/")
    target_file = target_file.replace("d:\\\\PRRO_GATE\\\\", "/mnt/d/PRRO_GATE/")
    target_file = target_file.replace("\\\\", "/")
    target_file = target_file.replace("\\", "/")
    
    chunks = args.get("ReplacementChunks", [])
    if not os.path.exists(target_file):
        print(f"Error: file not found {target_file}")
        return False
        
    with open(target_file, 'r', encoding='utf-8') as f:
        content = f.read()
        
    for chunk in chunks:
        target_content = chunk.get("TargetContent", "")
        replacement_content = chunk.get("ReplacementContent", "")
        if target_content in content:
            content = content.replace(target_content, replacement_content, 1)
        else:
            print(f"Warning: Chunk TargetContent not found in {target_file}")
            
    with open(target_file, 'w', encoding='utf-8') as f_out:
        f_out.write(content)
    print(f"Successfully applied multi-replacement to {target_file}")
    return True

# Read and collect tool calls in step order
collected_actions = []

with open(transcript_path, 'r', encoding='utf-8') as f:
    for line in f:
        try:
            step = json.loads(line)
            step_idx = step.get("step_index")
            if step_idx in steps_to_apply and step.get("source") == "MODEL" and "tool_calls" in step:
                for tc in step["tool_calls"]:
                    name = tc.get("name")
                    if name in ("replace_file_content", "multi_replace_file_content"):
                        collected_actions.append((step_idx, name, tc.get("args", {})))
        except Exception as e:
            continue

# Sort by step index so we apply changes in chronological order
collected_actions.sort(key=lambda x: x[0])

# Apply actions
for step_idx, name, args in collected_actions:
    print(f"Applying Step {step_idx} ({name})...")
    if name == "replace_file_content":
        apply_replace_file_content(args)
    elif name == "multi_replace_file_content":
        apply_multi_replace_file_content(args)

print("Done restoring.")
