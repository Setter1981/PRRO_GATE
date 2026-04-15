#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-find}"

if ! command -v jq >/dev/null 2>&1; then
  echo "Ошибка: нужен jq. Установи так:"
  echo "  sudo apt update && sudo apt install -y jq"
  exit 1
fi

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(pwd)}"

CANDIDATES=(
  "$PROJECT_DIR/.claude/settings.json"
  "$PROJECT_DIR/.claude/settings.local.json"
  "$HOME/.claude/settings.json"
  "$HOME/.claude/settings.local.json"
)

FOUND_ANY=0

print_header() {
  echo
  echo "============================================================"
  echo "$1"
  echo "============================================================"
}

show_hooks() {
  local file="$1"

  local has_stop
  has_stop="$(jq 'has("hooks") and (.hooks | has("Stop") or has("SubagentStop"))' "$file")"

  if [[ "$has_stop" != "true" ]]; then
    return 1
  fi

  FOUND_ANY=1
  print_header "Найден Stop/SubagentStop hook в: $file"

  echo "--- Stop ---"
  jq -C '.hooks.Stop // "нет"' "$file" || true

  echo
  echo "--- SubagentStop ---"
  jq -C '.hooks.SubagentStop // "нет"' "$file" || true

  return 0
}

disable_hooks() {
  local file="$1"

  local has_stop
  has_stop="$(jq 'has("hooks") and (.hooks | has("Stop") or has("SubagentStop"))' "$file")"

  if [[ "$has_stop" != "true" ]]; then
    return 1
  fi

  FOUND_ANY=1
  local backup="${file}.bak.$(date +%Y%m%d_%H%M%S)"

  cp "$file" "$backup"
  jq 'if has("hooks")
      then .hooks |= with_entries(select(.key != "Stop" and .key != "SubagentStop"))
      else .
      end' "$file" > "${file}.tmp"

  mv "${file}.tmp" "$file"

  print_header "Отключено в: $file"
  echo "Backup: $backup"
  echo "Удалены keys: Stop, SubagentStop"
}

print_header "Проверяем Claude Code settings"
echo "PROJECT_DIR = $PROJECT_DIR"
echo "MODE        = $MODE"

for file in "${CANDIDATES[@]}"; do
  if [[ -f "$file" ]]; then
    case "$MODE" in
      find)
        show_hooks "$file" || true
        ;;
      --disable|disable)
        disable_hooks "$file" || true
        ;;
      *)
        echo "Неизвестный режим: $MODE"
        echo "Использование:"
        echo "  $0            # только найти"
        echo "  $0 find       # только найти"
        echo "  $0 --disable  # отключить Stop/SubagentStop"
        exit 2
        ;;
    esac
  fi
done

if [[ "$FOUND_ANY" -eq 0 ]]; then
  print_header "Ничего не найдено"
  echo "Stop/SubagentStop hooks не найдены в типичных settings-файлах."
  echo "Проверь ещё managed/team settings или другой project root."
else
  print_header "Готово"
fi
