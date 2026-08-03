rbc 
kingdom_id          required int
x_coordinate      required  int
y_coordinate        required int
last_attacked       nullable int
last_defeated       nullable int
current_level       required int
times_defeated      default 0 int 

unique(kingdom_id, x_coordinate, y_coordinate)



attack
kingdom         int required
x_coordinate      required  int
y_coordinate        required int
march_id        int required
time_created    int
troop_count     nullable int 
duration        nullable int 
return_duration nullable int
coin_loot       nullable int
ruby_loot       nullable int

error_message   nullable text

