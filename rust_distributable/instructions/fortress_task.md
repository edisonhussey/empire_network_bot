currently the bot isnt able to identify fortress, 

we will need some structural changes to intialize this too. 

so beyond the radius from each kingdom castles. we will need to scan whole map 

fortress find one in initail gaa around castle in said castle. 

they appear always if exist +-17 , x +-17 y coordinate. 

so the algorithm will be to map right right right, .. until no more found, go to midddle logically then lef eft left,  . go middle. up upup. down down down. logically. 

then you will make these the bounding boxes, and requests gaa for every other not necessarily in perfect order. as long as get done with min number of requests. 

this will take even logner to initalize since we are adding limiting rate maybe avg 2.4 between call and +- some random distrubiton . 

the initalization happens to find the coordinates in each kingdom for fortress. guaranteed. 

...but we will need to scan those found in the kingdoms required where a task is fortress sassigned there to find out how long until it comes off cooldown. with similar intlai time. 

you will need to manage statemanatn of kingdom not just said out of context requests. i.e. transition kingdom request. in map mode




they can only be attacked initally with adi if already on cooldown. and must have high priority by default  so since its deteminsitic we can place in , and form the scheduler based on this being a must. 

if you allocated 4 commanders to fortress in sand and 5 appear at once, we only bother to send a hit iff at most 1 minute has passed since it became avaible.


track when your attack lands on target so another scheduled event. up to 1 - 30 minutes after your MID indicates it should land, and if loot rubies is none --> failed, and so read the real server truth cooldown and add to database, 
else rubies successful then you add 100 hr to the time you ladned at as the intnerl  truth of next cooldown time. 


they will have attached data, 24hrs coodown if someone else takes it
100 hr cooldown if you defeat one. 


