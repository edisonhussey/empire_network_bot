in recruit.py import named class type of troops. in game_Data. 

so i can write a config file 


once again accounts in config file subscripe to a recruit sequence. 

first task will be to deconstrcut a recruit instruction into where the field of troop id and qty match in game observed. 

it will require a castle id too maybe so let's have a look

a second param is quantity 



dont do what you dont figure out write your own files in /development_files of useful packets and ask questiosn if you dont know. 
i will capture the full set of instructions you need and we can break down slower 1 by 1 if you need. 
what latest captures show....
me in map mode -> go into my sands map. -> go to sands castle. -> recruit 190 crossbowmen one slot. ask help. exit recruit window -> go to open up main castle . end of sequence . gogogoog

------

config file:
------






event castle recruit is an object 

takes param 
-slot_quantity **how many separate recruit instructions per castle allowed
-ask_help  *bool* asks alliance help once the fujll slots are sent
-troop_id

[[[[ event castle recruit
received try to interpret the duration of first request i.e. 5 slots after first lot see return packet. then 
calculate total duration and subsec the min + 10s before scheduling that castle again. dynamic approach is best
i.e. 5 slots after first 
each castle has own independent timer in this case 5*duration of first request. since assume each slot has same 

]]]]
**must space out the requests in slow randomly timed out like 3.1 2.2 1.5 s etc.... maybe guassian around 3.1 min >= 0.5 






a recruit_task subscribes to castle recruit event and passes castle_id

i.e. sands kid =1 subscribed to a predefined castle event. like this 

task_crossbowman Task = {
    castle_id: recruit_task,
    castle_id: recruit_task
}

keep the last time the queue per castle clears in a datbase per account keep clean maybe as a epoch or datetime
must be compatible with bot drivers like task sands. so that scheduler can do both

VERY IMPORTANT
to keep a database var 
at all times to know where we 'are'

current_kingdom: an id value 
map_mode: False    (false = looking inside castle, true = ready to attack mode)
recruit_page: False/True (can only be true iff already map mode is false. i.e inside castle -> recruit page)

important because after recruitpage. we observe if cancelling request if not . next step would be to move to next task

We must understand how to move between castles. in map mode, or castle mode

i.e. castle sand castle -> castle id 0 is possible, but sand-> green also possible . 

keep in mind separate accounts own diff worlds. 







