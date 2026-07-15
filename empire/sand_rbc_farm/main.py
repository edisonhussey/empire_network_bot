from empire.network_sender.create_attack import * 
import random

HBW_VALUE = 1007 #coin travel 
STRICT_ATTACK_PACKET_COOLDOWN_MIN = 12.3
#requires an adi packet --> server first before send requests. 
#example packet = %xt%EmpireEx_21%cra%1%{"SX":593,"SY":613,"TX":597,"TY":613,"KID":1,"LID":0,"WT":0,"HBW":1007,"BPC":0,"ATT":0,"AV":0,"LP":0,"FC":0,"PTT":0,"SD":0,"ICA":0,"CD":99,"A":[{"L":{"T":[[-1,0],[-1,0]],"U":[[607,50],[-1,0]]},"R":{"T":[[-1,0],[-1,0]],"U":[[-1,0],[-1,0]]},"M":{"T":[[-1,0],[-1,0],[-1,0]],"U":[[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0]]}}],"BKS":[],"AST":[-1,-1,-1],"RW":[[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0]],"ASCT":0}%
#server to client bls command contains payloda values if successful - with the associated MID . this should be event based to update the countdown timer. 



#assume first 9 commanders are shield maiden and free to start with. 



#strict sequence is ADI -> 4-7 seconds loosly and random ---> cra packet ----> get MID get LID , track these internally. 
#DO NOT SEND OVERLY REQUESTS 
def rbc_cooldown_addition():
    return 3600 * 3 + 151 - random.randrange(-48, 81)
    
global_error_cooldown = 0 
consecutive_error_packet = 0

LEVEL_61 = Attack(
    wave1 = wave(
        left= side(
            tools=[],
            units = [(Troop.CROSSBOWMAN), 50]
        )
    )
)

NOT_LV_61 = Attack(
    wave1 = wave(
        left= side(
            tools=[],
            units = [(Troop.VALKYRIE_RANGER_10), 50]
        ),
        right = side(
            tools=[],
            units = [(Troop.VALKYRIE_RANGER_10), 50]
        ) 
    ),
    wave2 = wave(
        left= side(
            tools=[],
            units = [(Troop.VALKYRIE_RANGER_10), 50]
        ),
        right = side(
            tools=[],
            units = [(Troop.VALKYRIE_RANGER_10), 50]
        ) 
    ),
    wave3 = wave(
        left= side(
            tools=[],
            units = [(Troop.VALKYRIE_RANGER_10), 50]
        ),
        right = side(
            tools=[],
            units = [(Troop.VALKYRIE_RANGER_10), 50]
        ) 
    ),
    wave4 = wave(
        left= side(
            tools=[],
            units = [(Troop.VALKYRIE_RANGER_10), 50]
        ),
        right = side(
            tools=[],
            units = [(Troop.VALKYRIE_RANGER_10), 50]
        ) 
    ),
)


if __name__ == "__main__":
    ## assume you are in sand kingdom map view right now
    ## 

