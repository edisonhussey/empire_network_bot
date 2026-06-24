import time, random

class attack:
    
    # //formation is outlined in a folder.
    def __init__(self, time_sent, x, y, commander_id):
        outcome = False


class berimond_attack_manager:
    # ATTACK_COOLDOWN_INTERVAL = 5
    # COMMANDER_COUNT = 3
    # timeout_until = time.time() - 5
    # active_attacks = [time.time()] * COMMANDER_COUNT 
    
    
    def __init__(self):
        self.ATTACK_COOLDOWN_INTERVAL = 5
        self.COMMANDER_COUNT = 3
        self.timeout_until = time.time() - 5
        self.active_attacks = [time.time()] * self.COMMANDER_COUNT 
        self.last_attacked = time.time()
        self.return_speed_multiplier = 0.2
        

    def open_berimond_ui():
        #like this 
        # %xt%EmpireEx_19%klh%1%{}%
        # %xt%EmpireEx_19%pep%1%{"EID":3}%
        return
    
    def find_closest_watchtower():
        # %xt%EmpireEx_19%gbl%1%{}%
        # %xt%EmpireEx_19%klh%1%{}%
        return
    def attack(x, y):
        # %xt%EmpireEx_19%cra%1%{"SX":1280,"SY":46,"TX":1451,"TY":5,"KID":10,"LID":0,"WT":0,"HBW":1021,"BPC":0,"ATT":0,"AV":1,"LP":0,"FC":0,"PTT":0,"SD":0,"ICA":0,"CD":99,"A":[{"L":{"T":[[651,10],[-1,0]],"U":[[10,26],[-1,0]]},"R":{"T":[[-1,0],[-1,0]],"U":[[-1,0],[-1,0]]},"M":{"T":[[-1,0],[-1,0],[-1,0]],"U":[[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0]]}}],"BKS":[],"AST":[-1,-1,-1],"RW":[[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0]],"ASCT":0}%
        return
        
    
    def start(self):

            
        self.open_berimond_ui()
        

        
        
        
        # self.find_closest_watchtower()
        while True:
            time.sleep(2.5 * random.random())
            for attack_cooldown in self.active_attacks:
                if attack_cooldown < time.time() and time.time() - self.last_attacked > 4.5:
                    x,y = self.find_closest_watchtower()
                    self.attack(x,y)
                    # capture the attack that looks like this in utf 8 as a binary response
                    # its the cat type of msg 
                    
#the tt field is used in seconds 87 seconds here so you would add 87 + 87*0.2 = for time of flight and so we will naturally fier again only once aviaible. 
# listen for this msg after seidnng quests 
# %xt%cat%1%0%{"A":{"M":{"MID":727880143,"PT":0,"TT":87,"D":1,"TID":3116865,"T":2,"HBW":1007,"KID":0,"TA":[1,643,637,3139991,3116865,6,7,5,5,5,"abyss",0,0,-1,-1,-1,0,0,[],0],"SID":-209,"OID":3116865,"SA":[2,647,637,-1,9,10800,0]},"UM":{"PWD":0,"TWD":0,"L":{"ID":0,"WID":2,"VIS":0,"N":"npc 1","GID":104,"L":10,"ST":0,"W":10365,"D":37,"SPR":164,"EQ":[[3248171940,1,2,5,-1,[[4,100,[117.0]],[5,100,[77.0]],[106,100,[6.0]]],-1,-1,0,-1,-1,3,[1,7,4000,[750197,31,7,4000,[[201,100,[14.0]],[202,100,[49.0]],[205,100,[14.0]]],0]]],[3248169893,2,2,5,-1,[[1,100,[87.0]],[2,100,[87.0]],[103,100,[18.0]]],-1,-1,0,-1,-1,3,[2,7,4000,[750183,31,7,3510,[[206,87,[12.6]],[201,84,[12.3]],[208,80,[17.9]]],0]]],[3248170996,3,2,5,-1,[[3,100,[117.0]],[6,100,[77.0]],[101,100,[30.0]]],-1,-1,0,-1,-1,3,[3,7,4000,[750205,31,7,3470,[[205,80,[11.9]],[206,87,[12.6]],[203,80,[5.9]]],0]]],[3248170610,4,2,5,-1,[[7,100,[17.0]],[102,100,[30.0]],[107,100,[18.0]]],-1,-1,0,-1,-1,3,[4,7,4000,[750267,31,7,4000,[[204,100,[7.0]],[208,100,[21.0]],[202,100,[49.0]]],0]]],[3250114874,6,2,15,-1,[[816,67,[17.4]],[812,83,[29.9]],[810,77,[44.9]],[809,57,[36.1]],[20015,37,[22,35.0,23,35.0]],[20018,97,[1.0]]],-1,-1,0,-1,-1,3,[65,7,3840,[]]]],"GASAIDS":[],"SIDS":[104022],"AE":[[426,[10.0],"GE"],[97,[628,6.0,630,6.0,631,5.0,636,5.0],"RH"]]}},"A":[[664,9]],"G":[["W",82],["S",61],["F",162],["C1",268]],"S":0},"O":[{},{"OID":3116865,"DUM":false,"N":"Furox","E":{"BGT":0,"BGC1":14408394,"BGC2":14408394,"SPT":1,"S1":44,"SC1":1644825,"S2":44,"SC2":1644825,"IS":1},"L":70,"LL":950,"H":472,"AVP":87840,"CF":2277817,"HF":331348383,"PRE":13,"SUF":16,"TOPX":-1,"MP":1502292,"R":0,"AID":57949,"AR":9,"AN":"Achilles Legion","aee":{"ACCA":{"ACLI":7,"ACCS":[11,1]},"ACFB":{}},"RPT":0,"AP":[[0,3139991,643,637,1],[1,3225349,690,669,12],[2,3223795,636,654,12],[3,3231874,697,580,12],[4,1761,632,582,12]],"VP":[],"SA":0,"VF":0,"PF":1,"RRD":0,"TI":-1,"FN":{"MC":-1,"FID":1,"TID":103,"NS":-1,"PMS":-1,"PMT":0,"SPC":0}}]}%
                    #if error at this stage or 2 second timeout with no response from server , consider it a failed attack backoff for 30-40 seconds and go to greenkingdom again , and do start again. if more than 4 afiled attacks in a row then you will termiante script and debug.
                    



















        ##find the watchtower somehow and get its coords to then use this attack setup verbatum except acc id and commander id would differ 
        # %xt%EmpireEx_19%cra%1%{"SX":1280,"SY":46,"TX":1451,"TY":5,"KID":10,"LID":0,"WT":0,"HBW":1021,"BPC":0,"ATT":0,"AV":1,"LP":0,"FC":0,"PTT":0,"SD":0,"ICA":0,"CD":99,"A":[{"L":{"T":[[651,10],[-1,0]],"U":[[10,26],[-1,0]]},"R":{"T":[[-1,0],[-1,0]],"U":[[-1,0],[-1,0]]},"M":{"T":[[-1,0],[-1,0],[-1,0]],"U":[[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0]]}}],"BKS":[],"AST":[-1,-1,-1],"RW":[[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0],[-1,0]],"ASCT":0}%
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        #ignore
    # def send_over_trops()
    #ignore this for now ---- %xt%EmpireEx_19%kut%1%{"SCID":3139991,"SKID":0,"TKID":10,"CID":-1,"A":[[10,302],[651,5129]]}%
                
