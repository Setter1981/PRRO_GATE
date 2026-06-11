#!/bin/bash
TRANSCRIPT_PATH="/mnt/c/Users/Користувач/.gemini/antigravity/brain/de409f46-3e36-4403-95eb-de9a6462bfdb/.system_generated/logs/transcript.jsonl"
grep -E '"name":"(replace_file_content|write_to_file|multi_replace_file_content)"' "$TRANSCRIPT_PATH" | while read -r line; do
    echo "STEP: $(echo "$line" | grep -oE '"step_index":[0-9]+')"
    echo "TOOL: $(echo "$line" | grep -oE '"name":"[^"]+"')"
    echo "ARGS: $(echo "$line" | grep -oE '"args":\{[^}]+\}')"
    echo "---"
done
