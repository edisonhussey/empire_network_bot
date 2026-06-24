



# class veteran_demon_horror



class Troop:
    def __init__(self ,troop_id, name, attack_power, melee_defence, ranged_defence, loot_capacity, travel_speed, food_consumption):
        self.troop_id = troop_id
        self.name = name
        self.attack_power = attack_power
        self.melee_defence = melee_defence
        self.ranged_defence = ranged_defence
        self.loot_capacity = loot_capacity
        self.travel_speed = travel_speed
        self.food_consumption = food_consumption

def create_troop_data():
    troop_data: dict = {}
    
    troop_data[9] = Troop(9, "veteran_demon_horror", 200, 21, 6, 45, 30, 5)
    
    #demon_horror
    troop_data[714] = Troop(
        troop_id=714,
        name="demon_horror",
        attack_power=185,      # Red sword (main attack)
        melee_defence=19,      # Blue sword
        ranged_defence=5,      # Bow
        loot_capacity=35,      # Bag icon
        travel_speed=28,       # Hourglass (movement / time)
        food_consumption=5     # Bread (upkeep)
    )
    
    troop_data[10] = Troop(
        troop_id=10,
        name="veteran_deathly_horror",
        attack_power=175,      # Red bow (ranged attack)
        melee_defence=17,      # Blue sword
        ranged_defence=26,     # Blue bow
        loot_capacity=50,      # Bag
        travel_speed=30,       # Hourglass
        food_consumption=5     # Bread
    )
    
        # === NEW TROOP - Renegade Kunai Thrower ===
    troop_data[35] = Troop(
        troop_id=35,
        name="renegade_kunai_thrower",
        attack_power=148,      # Red bow (ranged attack)
        melee_defence=18,      # Blue sword
        ranged_defence=27,     # Blue bow
        loot_capacity=25,      # Bag
        travel_speed=50,       # Hourglass
        food_consumption=4     # Bread
    )
    
    troop_data[664] = Troop(
        troop_id=664,
        name="crossbowman_of_the_kingsguard",
        attack_power=121,      # Red bow
        melee_defence=14,      # Blue sword
        ranged_defence=23,     # Blue bow
        loot_capacity=29,      # Bag
        travel_speed=28,       # Hourglass
        food_consumption=4     # Bread
    )
    
    troop_data[999] = Troop(          # ← Change 999 when you see the real ID
        troop_id=999,
        name="shield_maiden",
        attack_power=225,      # Red sword (melee attack)
        melee_defence=28,      # Blue sword
        ranged_defence=10,     # Blue bow
        loot_capacity=43,      # Bag
        travel_speed=34,       # Hourglass
        food_consumption=2     # Boots + oil? (upkeep)
    )
    
    return troop_data
    
        
# class Attack:
    
    
