import json
from pathlib import Path

def _append_brain_log(state_root: Path, persona_date: str, record: dict) -> None:
    path = Path(state_root) / "brain" / f"{persona_date}.ndjson"
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as f:
        f.write(json.dumps(record, ensure_ascii=False) + "\n")
