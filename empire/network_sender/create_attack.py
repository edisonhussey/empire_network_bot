from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


if __package__ in {None, ""}:
    repo_root = Path(__file__).resolve().parents[2]
    sys.path.insert(0, str(repo_root))
    from empire.attacks.attack import Attack, side, wave
    from empire.attacks.kingdom import Kingdom
    from empire.attacks.troops import Troop
    from empire.attacks.tools import Tool
else:
    from ..attacks.attack import Attack, side, wave
    from ..attacks.kingdom import Kingdom
    from ..attacks.troops import Troop
    from ..attacks.tools import Tool


REPO_ROOT = Path(__file__).resolve().parents[2]
PROXY_SEND_FILE = REPO_ROOT / "captures" / "inject3_send.txt"

DEFAULT_ATTACK = Attack(
    # wave1=wave(
    #     middle=side(
    #         tools=[(Tool.SCALING_LADDER, 10)],
    #         units=[(Troop.VALKYRIE_RANGER_0, 15)],
    #     )
    # ),
    # wave2=wave(
    #     left=side(
    #         tools=[(Tool.SCALING_LADDER, 10)],
    #         units=[(Troop.VALKYRIE_RANGER_0, 5)],
    #     )
    # ),
    wave1 = wave(
        left= side(
            tools=[],
            units = [(Troop.DEMON_HORROR), 50]
        )
    )
)


def enum_id(value: Any) -> int:
    return value.id if hasattr(value, "id") else int(value)


@dataclass
class AttackRequest:
    SX: int
    SY: int
    TX: int
    TY: int
    KID: Kingdom | int
    LID: int
    attack: Attack

    WT: int = 0
    HBW: int = 1007
    BPC: int = 0
    ATT: int = 0
    AV: int = 0
    LP: int = 0
    FC: int = 0
    PTT: int = 0
    SD: int = 0
    ICA: int = 0
    CD: int = 99
    BKS: list[Any] = field(default_factory=list)
    AST: list[int] = field(default_factory=lambda: [-1, -1, -1])
    RW: list[list[int]] = field(default_factory=lambda: [[-1, 0] for _ in range(8)])
    ASCT: int = 0


ATTACK_REQUEST = AttackRequest(
    SX=697,
    SY=580,
    TX=698,
    TY=581,
    KID=Kingdom.FIRE_KINGDOM,
    LID=10,
    attack=DEFAULT_ATTACK,
)


def _create_attack_payload(request: AttackRequest) -> dict[str, Any]:
    return {
        "SX": request.SX,
        "SY": request.SY,
        "TX": request.TX,
        "TY": request.TY,
        "KID": enum_id(request.KID),
        "LID": request.LID,
        "WT": request.WT,
        "HBW": request.HBW,
        "BPC": request.BPC,
        "ATT": request.ATT,
        "AV": request.AV,
        "LP": request.LP,
        "FC": request.FC,
        "PTT": request.PTT,
        "SD": request.SD,
        "ICA": request.ICA,
        "CD": request.CD,
        "A": request.attack.to_payload(),
        "BKS": request.BKS,
        "AST": request.AST,
        "RW": request.RW,
        "ASCT": request.ASCT,
    }


def create_attack(
    *,
    request: AttackRequest,
    server_header: str = "EmpireEx_19",
    request_id: int = 1,
) -> str:
    payload = _create_attack_payload(request)
    json_payload = json.dumps(payload, separators=(",", ":"))
    return f"%xt%{server_header}%cra%{request_id}%{json_payload}%"


def queue_attack(
    *,
    request: AttackRequest,
    send_file: Path = PROXY_SEND_FILE,
    server_header: str = "EmpireEx_19",
    request_id: int = 1,
) -> str:
    packet = create_attack(
        request=request,
        server_header=server_header,
        request_id=request_id,
    )
    send_file.parent.mkdir(parents=True, exist_ok=True)
    with send_file.open("a", encoding="utf-8") as handle:
        handle.write(f"{packet}\n")
    return packet


if __name__ == "__main__":
    packet = queue_attack(request=ATTACK_REQUEST)
    print(f"queued attack packet -> {PROXY_SEND_FILE}")
    print(packet)
