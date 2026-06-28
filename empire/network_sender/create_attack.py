from __future__ import annotations

import sys
from pathlib import Path


if __package__ in {None, ""}:
    repo_root = Path(__file__).resolve().parents[2]
    sys.path.insert(0, str(repo_root))
    from empire.attacks.kingdom import Kingdom
    from empire.attacks.troops import Troop
    from empire.attacks.tools import Tool
else:
    from ..attacks.kingdom import Kingdom
    from ..attacks.troops import Troop
    from ..attacks.tools import Tool


json_payload = "hi"


def create_attack(SX: int, SY: int, TX: int, TY: int, KID: int, LID: int):
    #wt = unknown field bpc unknown att unkonwn
    
    return


prefix = f"%xt%EmpireEx_19%cra%1%{json_payload}%"


if __name__ == "__main__":
    print(Troop.SHIELD_MAIDEN_0.id)
    print(Kingdom.GREEN_KINGDOM.id)
# printf '%s\n' '%xt%EmpireEx_19%cra%1%{"SX":643,"SY":637,"TX":644,"TY":639,"KID":0,"LID":10,"WT":0,"HBW":1007,"BPC":0,"ATT":0,"AV":0,"LP":0,"FC":0,"PTT":0,"SD":0,"ICA":0,"CD":99,"A":[{"L":{"T":[[-1,0],[-1,0]],"U":[[-1,0],[-1,0]]},"R":{"T":[[-1,0],[-1,0]],"U":[[-1,0],[-1,0]]},"M":{"T":[[-1,0],[-1,0],[-1,0]],"U":[[215,28],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0]]}}],"BKS":[],"AST":[-1,-1,-1],"RW":[[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0]],"ASCT":0}%' >> captures/inject3_send.txt
