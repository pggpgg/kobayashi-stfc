# Building bonus mapping gaps

Directory: `/Users/pgagnong/Dev/kobayashi-stfc/data/buildings`

## Opaque `buff_*` stats

These keys are not merged into the player combat profile (see `merge_building_bonuses_into_profile` / `normalize_profile_combat_stat` in `src/data/profile.rs`). Descriptions are from stfc.space / game translations (`starbase_module_buff_description`) matched via `loca_id` in each bonus’s `notes` field.

| Stat | Description | Building name(s) |
| --- | --- | --- |
| `buff_1004648085` | The Repair Speed for the ship that is affiliated to this Drydock is increased every time you upgrade it. | Drydock G |
| `buff_1019151668` | The Maximum amount of Σ-Dilithium you can stock from the Generators is increased each time you upgrade the Dilithium Warehouse. | Dilithium Warehouse |
| `buff_1023059691` | Increases the maximum number of ships that can take part in your Assaults against enemy Alliance Starbases. | Command Control |
| `buff_1029146233` | Enables the detection of cloaked ships when an Assault occurs against your Alliance Starbase.   Only cloaked ships of the assaulting Alliance are detected inside the same system as your Alliance Starbase. | Tachyon Detector |
| `buff_1031293542` | Storage indicates the base amount of Material Fragments that can be held by the Outpost Control Center before getting full and needing to be collected. This increases each time you upgrade the building. | Outpost Control Center |
| `buff_1034040906` | The amount of Active Plasma which can be stored within this harvester. | Active Plasma Harvester B |
| `buff_1067035410` | The Dilithium Storage indicates how much Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator B |
| `buff_1085523180` | The amount of Σ-Tritanium that cannot be stolen when your station gets attacked is increased each time you upgrade the Tritanium Vault. | Tritanium Vault |
| `buff_1088903989` | Additional Fleet Commander slots are unlocked.  Slot 1: Command Center level 1 Slot 2: Command Center level 40 | Command Center |
| `buff_1121407171` | Increases the warp range of the Alliance Starbase. | Assembly Chambers |
| `buff_1137049964` | Increases the Weapon Damage of all alliance members ships when attacking Cardassian Stations. | Tactical Deck |
| `buff_1145531007` | The Tritanium Storage indicates how much Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator A |
| `buff_1152756547` | Increases Cost Efficiency of Sigma [Σ] Parsteel, Sigma [Σ] Tritanium, and Sigma [Σ] Dilithium for all Buildings | INDEPENDENT ARCHIVES |
| `buff_1156618065` | The Protomatter rewarded from completing a Trial is increased each time you upgrade the Court of Q building. | Court of Q |
| `buff_1200315561` | Additional Exocomps are unlocked. Exocomps can activate Consumables. Level 1: +1 Galaxy Exocomp Level 5: +1 Station Exocomp Level 15: +1 Combat Exocomp Level 35: +1 Multi-purpose Exocomp | Exocomp Factory |
| `buff_1210413935` | The Σ-Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator C |
| `buff_1228706224` | The Σ-Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator E |
| `buff_1232374297` | Increases the Shield Health of the Alliance Starbase. | Shield Modulator |
| `buff_1233401306` | The Parsteel Storage indicates how much Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator B |
| `buff_1240792636` | The Repair Speed for the ship that is affiliated to this Drydock is increased every time you upgrade it. | Drydock D |
| `buff_1258794338` | The amount of Active Plasma which can be stored within this harvester. | Active Plasma Harvester C |
| `buff_1260565637` | The base Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator F |
| `buff_1277421908` | Increases base Rare Transogen Mining rate. | Transogen Forge |
| `buff_1287243827` | The base Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator G |
| `buff_1293910107` | A one time reward of Uncommon Skill Points is granted each time you upgrade the Command Center. | Command Center |
| `buff_1311541559` | The Σ-Tritanium Storage indicates how much Σ-Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator H |
| `buff_1322427309` | Increases Cost Efficiency of Parsteel, Tritanium, and Dilithium for all Buildings | INDEPENDENT ARCHIVES |
| `buff_1361889997` | The base Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator B |
| `buff_1384123882` | The maximum cargo capacity of all ships is increased when the Treasury is upgraded. | Treasury |
| `buff_1393830736` | Increase the Damage of the Alliance Starbase. | EPS Distributor C |
| `buff_1394822749` | The Σ-Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator C |
| `buff_1400171833` | The Repair Costs for the ship that is affiliated to this Drydock is decreased every time you upgrade it. | Drydock C |
| `buff_1422729787` | Reduces opponent ship's Hyperthermic Decay against Aggregation hostiles | Recon Locus |
| `buff_1443149197` | The Tritanium Storage indicates how much Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator F |
| `buff_1461707368` | The mining speed of Survey ships is increased when the Treasury is upgraded. | Treasury |
| `buff_150353432` | Increases Cost Efficiency of Tiering up Chaos Tech | INDEPENDENT ARCHIVES |
| `buff_1509056673` | The base Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator H |
| `buff_156215254` | The amount of Σ-Parsteel that cannot be stolen when your station gets attacked is increased each time you upgrade the Parsteel Vault. | Parsteel Vault |
| `buff_1584895739` | Defense Platforms protect your Station from other players' attacks. | Defense Platform A |
| `buff_1587727214` | The Σ-Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator B |
| `buff_1593096695` | Increases base positive FKR reputation received from Hostiles and Armadas each time you upgrade the Holodeck building. | Holodeck |
| `buff_1601530866` | The Σ-Tritanium Storage indicates how much Σ-Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator A |
| `buff_1605117396` | Defense Platforms protect your Station from other players' attacks. | Defense Platform B |
| `buff_1655935815` | Increases base Damage against Outposts and their Retaliation ships. | Outpost Control Center |
| `buff_1656851848` | Increases base rewards obtained from claiming Warchests (approximative, average increase) | The War Room |
| `buff_1673044410` | Defense Platforms protect your Station from other players' attacks. | Defense Platform D |
| `buff_1692645168` | The Weapon Damage dealt by your Defense Platforms is increased each time you upgrade the Defense Technologies building. | Defense Technologies |
| `buff_1695075041` | The Σ-Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator G |
| `buff_1712157842` | The Repair Costs for the ship that is affiliated to this Drydock is decreased every time you upgrade it. | Drydock A |
| `buff_1725504841` | The amount of Collisional Plasma that is protected when your Alliance Starbase is attacked. | Collisional Plasma Vault B |
| `buff_1738545464` | The Σ-Parsteel Storage indicates how much Σ-Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator A |
| `buff_1764042681` | The Dilithium Storage indicates how much Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator A |
| `buff_1768483334` | The Repair Speed for the ship that is affiliated to this Drydock is increased every time you upgrade it. | Drydock F |
| `buff_1786572787` | The amount of Dilithium that cannot be stolen when your station gets attacked is increased each time you upgrade the Dilithium Vault. | Dilithium Vault |
| `buff_1789222968` | The Maximum amount of Tritanium you can stock from the Generators is increased each time you upgrade the Tritanium Warehouse. | Tritanium Warehouse |
| `buff_179190365` | The Parsteel Storage indicates how much Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator H |
| `buff_1838839573` | Increases Warp Speed for all ships | Subspace Relay |
| `buff_1847290680` | Defense Platforms protect your Station from other players' attacks. | Defense Platform E |
| `buff_1847534232` | Increases the cost efficiency of Service Awards when used for research. | Shuttle Bay |
| `buff_1883114020` | The Σ-Dilithium Storage indicates how much Σ-Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator F |
| `buff_189369985` | Increase the Damage of the Alliance Starbase. | EPS Distributor B |
| `buff_189836070` | The Repair Costs for the ship that is affiliated to this Drydock is decreased every time you upgrade it. | Drydock F |
| `buff_1911599164` | The Parsteel Storage indicates how much Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator F |
| `buff_1961479139` | The Parsteel Storage indicates how much Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator D |
| `buff_2015738784` | The Repair Costs for the ship that is affiliated to this Drydock is decreased every time you upgrade it. | Drydock E |
| `buff_2026743585` | The base Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator H |
| `buff_2028595564` | The amount of Magnetic Plasma that can be harvested per hour. | Magnetic Plasma Harvester B |
| `buff_2092352680` | The amount of Collisional Plasma which can be stored within this harvester. | Collisonal Plasma Harvester C |
| `buff_2105146395` | Increases base speed of constructing Ships | Recon Locus |
| `buff_2132727674` | The Dilithium Storage indicates how much Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator C |
| `buff_2151753795` | Increases base Cost Efficiency of Ship Parts in ship components each time you upgrade the Holodeck building. | Holodeck |
| `buff_2180825491` | The Σ-Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator D |
| `buff_2185834164` | The amount of Active Plasma that is protected when your Alliance Starbase is attacked. | Active Plasma Vault B |
| `buff_2201489961` | When upgrading The Nova Squadron you will receive Nova Squadron particles, which can be used to unlock and upgrade:  NS Burning Isolytic Damage NS Morale Isolytic Damage NS HB Isolytic Damage NS Burning Mitigation NS Morale Mitigation NS HB Mitigation NS Burning Damage NS Morale SHP NS HB Crit Damage Reduction | The Nova Squadron |
| `buff_2221457125` | The Construction Speed for all ships you construct is increased every time you upgrade the Shipyard. | Shipyard |
| `buff_2235614043` | The base Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator E |
| `buff_2264815824` | The grade of Exocomp Consumables that can be purchased in the Consumables Store is increased. | Exocomp Factory |
| `buff_2270080811` | The base Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator C |
| `buff_2291525316` | The Tritanium Storage indicates how much Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator D |
| `buff_2313460695` | The amount of Active Plasma that can be processed by the Alliance Starbase before it needs to relocate to a different Plasma Storm. It is increased each time it is upgraded. | Plasma Processor |
| `buff_2327416739` | The Repair Costs for the ship that is affiliated to this Drydock is decreased every time you upgrade it. | Drydock G |
| `buff_2334409927` | Increases the Armor Piercing, Accuracy and Shield Piercing of the Alliance Starbase. | Exographic Targeting Array A |
| `buff_234221410` | The base Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator D |
| `buff_2365854482` | The Σ-Tritanium Storage indicates how much Σ-Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator B |
| `buff_2383989383` | The Σ-Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator A |
| `buff_2389523082` | Upgrade the Signal Observatory to increase the FKR Credits earned from the V'ger Challenge Track.  Level 20: 10%  Level 30: 15%  Level 40: 20%  Level 50: 25%  Level 60: 30%  Level 70: 40%  Level 75: 50% | Signal Observatory |
| `buff_240592581` | The Parsteel Storage indicates how much Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator E |
| `buff_2437824470` | The base Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator E |
| `buff_2439973925` | Increases the Armor Piercing, Accuracy and Shield Piercing of the Alliance Starbase. | Exographic Targeting Array C |
| `buff_2467916106` | The Dilithium Storage indicates how much Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator G |
| `buff_2472694527` | The amount of Active Plasma that is protected when your Alliance Starbase is attacked. | Active Plasma Vault A |
| `buff_2485849598` | Increases the maximum number of ships that can take part in your Armada attacks on Hostile targets. | Armada Control Center |
| `buff_2489366466` | Increases the Shield Health of the Alliance Starbase. | Shield Modulator |
| `buff_2571750603` | The Repair Costs for the ship that is affiliated to this Drydock is decreased every time you upgrade it. | Drydock H |
| `buff_2591204557` | The amount of Artifact Tokens gained from defeating Formation Armadas is increased each time you upgrade the Artifact Gallery. | Artifact Gallery |
| `buff_2594495003` | Increases the maximum number of ships that can take part in your Open Armada attacks on Hostile targets. | Armada Control Center |
| `buff_26008808` | The Σ-Tritanium Storage indicates how much Σ-Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator G |
| `buff_2608024101` | Increase the Damage of the Alliance Starbase. | EPS Distributor A |
| `buff_2619837429` | The Maximum amount of Parsteel you can stock from the Generators is increased each time you upgrade the Parsteel Warehouse. | Parsteel Warehouse |
| `buff_2652133459` | The amount of Magnetic Plasma that is protected when your Alliance Starbase is attacked. | Magnetic Plasma Vault A |
| `buff_2654147167` | The Σ-Tritanium Storage indicates how much Σ-Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator F |
| `buff_2665414430` | Unlocks Maverick Task Key claim in the Maverick Faction store. | The Warp Dive Bar |
| `buff_266735500` | The Σ-Parsteel Storage indicates how much Σ-Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator H |
| `buff_26768571` | The amount of Collisional Plasma that can be harvested per hour. | Collisonal Plasma Harvester A |
| `buff_2678382465` | The Σ-Parsteel Storage indicates how much Σ-Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator E |
| `buff_2708204242` | The Σ-Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator F |
| `buff_2717945152` | The amount of Active Plasma which can be stored within this harvester. | Active Plasma Harvester A |
| `buff_2741881875` | The amount of Collisional Plasma which can be stored within this harvester. | Collisonal Plasma Harvester A |
| `buff_2744202021` | The amount of Axionic Chips that can be claimed daily in the Consumables Store is increased. | Exocomp Factory |
| `buff_2769496474` | The Repair Speed for the ship that is affiliated to this Drydock is increased every time you upgrade it. | Drydock C |
| `buff_2779204893` | The Σ-Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator B |
| `buff_2786406413` | Increases the cost efficiency of Crystal, Gas, and Ore when used for research. | Refinery |
| `buff_2830549382` | Increases the base speed of Research | Recon Locus |
| `buff_2841294787` | The Σ-Dilithium Storage indicates how much Σ-Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator E |
| `buff_2886402254` | The base Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator G |
| `buff_2919223790` | The amount of Active Plasma that can be harvested per hour. | Active Plasma Harvester C |
| `buff_2993973268` | The Σ-Dilithium Storage indicates how much Σ-Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator H |
| `buff_3005330147` | Upgrade the Signal Observatory to obtain Challenge Track Gifts.   Level 10, Level 20, Level 30, Level 40, Level 50, Level 60, Level 70, Level 80 | Signal Observatory |
| `buff_301108821` | Increases FKR Isolytic Damage against Conqueror Borg Solo Armadas. | The Warp Dive Bar |
| `buff_3029261688` | The base Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator C |
| `buff_3053201411` | The Repair Speed for the ship that is affiliated to this Drydock is increased every time you upgrade it. | Drydock A |
| `buff_3058492167` | The Tritanium Storage indicates how much Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator C |
| `buff_3060982158` | The amount of Magnetic Plasma that can be processed by the Alliance Starbase before it needs to relocate to a different Plasma Storm. It is increased each time it is upgraded. | Plasma Processor |
| `buff_3069298570` | Increases daily Armada Countdown Speed Ups granted by the Armada Quick Start favor in the Section 31 faction store. | The Facade |
| `buff_3070785839` | Grants the Daily V'ger Bounty Challenge for free each day. | Signal Observatory |
| `buff_3072710486` | The Tritanium Storage indicates how much Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator G |
| `buff_3089227077` | The amount of Collisional Plasma that can be harvested per hour. | Collisonal Plasma Harvester C |
| `buff_3120953072` | Increases Base Damage vs V'ger [VGER] Hostiles. | Signal Observatory |
| `buff_3123707629` | The Σ-Dilithium Storage indicates how much Σ-Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator B |
| `buff_3129421262` | Increases Research Speed each time you upgrade the Mess Hall. | Mess Hall |
| `buff_3134155780` | The base Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator A |
| `buff_3138264717` | Increases the Armor Piercing, Accuracy and Shield Piercing of the Alliance Starbase. | Exographic Targeting Array B |
| `buff_3140509951` | The Tritanium Storage indicates how much Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator E |
| `buff_3143956245` | The Parsteel Storage indicates how much Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator C |
| `buff_3154267878` | Increases Defense Platform Damage. | Subspace Relay |
| `buff_3185157131` | The Σ-Dilithium Storage indicates how much Σ-Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator D |
| `buff_3192800019` | Increases your Outpost Fleet size by 2 at Level 1, and by 1 at Level 40. | Outpost Control Center |
| `buff_3206125489` | The Repair Costs for the ship that is affiliated to this Drydock is decreased every time you upgrade it. | Drydock B |
| `buff_3226994667` | Increases base Common Transogen Mining rate. | Transogen Forge |
| `buff_322840403` | The amount of Magnetic Plasma which can be stored within this harvester. | Magnetic Plasma Harvester A |
| `buff_325303903` | The Maximum amount of Σ-Parsteel you can stock from the Generators is increased each time you upgrade the Parsteel Warehouse. | Parsteel Warehouse |
| `buff_3267742511` | The base Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator G |
| `buff_3287388779` | The Cost Efficiency of all Research for base resources is increased | Astronautics Studio |
| `buff_3287566361` | Increases the amount of Alliance Reputation that is gained when defeating Cardassian Stations. | Diplomatic Relations |
| `buff_3309277866` | Resets the cooldown and increase rewards of the <color=#ffc926>District 56</color> free claim in the <color=#FFC926>Mirror Refinery</color> | District 56 |
| `buff_3318715228` | Increases the base amount of Broken Ship Parts dropped from hostiles | DTI Headquarters |
| `buff_3333967977` | Increases the cost efficiency of Merits of Honor when used for research. | Shuttle Bay |
| `buff_3376274396` | The Σ-Parsteel Storage indicates how much Σ-Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator F |
| `buff_3383388089` | The base Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator A |
| `buff_3401661757` | Increases the Weapon Damage of your ships when they take part in an Armada attack on a Hostile target. | Armada Control Center |
| `buff_341559099` | Base Damage increased when fighting Krenim enemies | DTI Headquarters |
| `buff_3437453676` | Unlocks a free daily claim of Archive Shards in the Building Resources section in Gifts | INDEPENDENT ARCHIVES |
| `buff_3450405443` | The base Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator H |
| `buff_3451685863` | Defense Platforms protect your Station from other players' attacks. | Defense Platform C |
| `buff_3452562264` | The Σ-Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator H |
| `buff_3454214310` | The Σ-Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator D |
| `buff_3461716896` | The Σ-Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator H |
| `buff_3465507458` | The Σ-Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator A |
| `buff_3470553419` | The Scrapping Speed for all ships you scrap is increased every time you upgrade the Scrapyard. | Scrapyard |
| `buff_3493826101` | Increases the Armor, Dodge and Shield Deflection of the Alliance Starbase. | Shield Modulator |
| `buff_3505066034` | The Σ-Tritanium Storage indicates how much Σ-Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator C |
| `buff_3539540702` | The Repair Speed for the ship that is affiliated to this Drydock is increased every time you upgrade it. | Drydock H |
| `buff_3565741267` | The amount of Active Plasma that can be harvested per hour. | Active Plasma Harvester B |
| `buff_3569905798` | The Σ-Parsteel Storage indicates how much Σ-Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator G |
| `buff_3588567631` | The base Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator A |
| `buff_3590595484` | The Σ-Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator B |
| `buff_3608744035` | Increases the Hull Health of the Alliance Starbase. | Command Control |
| `buff_3629313671` | Capacity indicates the base amount of Material Fragments that can be stored at the station in total. Once reaching this cap the Outpost Control Center will stop generating Material Fragments. This increases each time you upgrade the building. | Outpost Control Center |
| `buff_3637014985` | The amount of Parsteel that cannot be stolen when your station gets attacked is increased each time you upgrade the Parsteel Vault. | Parsteel Vault |
| `buff_3647094777` | The base Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator C |
| `buff_3650968419` | Increases base Cost Efficiency of 7⇵ Uncommon Ore, Gas, and Crystal for Station buildings. | Signal Observatory |
| `buff_366051680` | The amount of Collisional Plasma that is protected when your Alliance Starbase is attacked. | Collisional Plasma Vault A |
| `buff_3670025699` | Borg Type 03 Solo Armadas found near <link="fleetcommand://link/navigation/galaxy?ID=806106205"><color=#3db4cc>Veridian</color></link> can only be fought with 1 ship instead of 3 due to Q's Interference. | Armada Control Center |
| `buff_371097431` | The Σ-Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator G |
| `buff_3725874398` | Increases base Cost Efficiency of Section 31 Transmitters, Emblems of Assessment, Quantum Communicators, Subspace Relay Upgrades, and Holo-Field Amplifiers. | The Nova Squadron |
| `buff_3729540977` | Increases Tritanium Cost Efficiency for ship components each time you upgrade the Mess Hall. | Mess Hall |
| `buff_3746868660` | Increases the cost efficiency of Crystal, Gas, and Ore when used for buildings. | Refinery |
| `buff_375224280` | The Repair Speed for the ship that is affiliated to this Drydock is increased every time you upgrade it. | Drydock B |
| `buff_3814633593` | The Σ-Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator G |
| `buff_3868819963` | The Σ-Tritanium Storage indicates how much Σ-Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator D |
| `buff_3870591038` | The amount of Magnetic Plasma that can be harvested per hour. | Magnetic Plasma Harvester A |
| `buff_3891125844` | The Σ-Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator A |
| `buff_3908773390` | The Dilithium Storage indicates how much Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator D |
| `buff_3914539627` | The Repair Speed for the ship that is affiliated to this Drydock is increased every time you upgrade it. | Drydock E |
| `buff_3940777734` | The Inventory indicates how many ships you can have in total, including ones that are not affiliated to a Drydock. | Ship Hangar |
| `buff_3945032962` | The Σ-Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator E |
| `buff_3980673100` | The amount of Collisional Plasma which can be stored within this harvester. | Collisonal Plasma Harvester B |
| `buff_4007778307` | The protected cargo size of Survey ships is increased when the Treasury is upgraded. | Treasury |
| `buff_4016535201` | The amount of Collisional Plasma that can be harvested per hour. | Collisonal Plasma Harvester B |
| `buff_4020372601` | Increases base HHP for all ships. | The Facade |
| `buff_4020430113` | Increases base Parsteel and Σ-Parsteel cost efficiency for buildings. | The Warp Dive Bar |
| `buff_4026284240` | The amount of Magnetic Plasma that can be harvested per hour. | Magnetic Plasma Harvester C |
| `buff_4054016168` | The base Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator D |
| `buff_4076943075` | The Dilithium Storage indicates how much Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator E |
| `buff_4081135111` | The Armor, Shield Deflection and Dodge of your Defense Platforms is increased each time you upgrade the Defense Technologies building. | Defense Technologies |
| `buff_4096145473` | The Parsteel Storage indicates how much Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator A |
| `buff_4102292780` | Increases Cost Efficiency of Leveling up Chaos Tech | INDEPENDENT ARCHIVES |
| `buff_4102916123` | The Σ-Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator C |
| `buff_4147144843` | The base Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator B |
| `buff_4159642322` | The amount of Σ-Dilithium that cannot be stolen when your station gets attacked is increased each time you upgrade the Dilithium Vault. | Dilithium Vault |
| `buff_4204537705` | The Σ-Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator E |
| `buff_4227450471` | The Tier Up Speed for all ships you upgrade is increased every time you upgrade the Shipyard. | Shipyard |
| `buff_4231534059` | Production indicates the base amount of Material Fragments generated per hour by the Outpost Control Center. This amount increases each time you upgrade the building. | Outpost Control Center |
| `buff_4233854918` | The Parsteel Storage indicates how much Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator G |
| `buff_4257724198` | The Shield Health of all your Battleships is increased each time you upgrade the Foundry. | Foundry |
| `buff_430522112` | Increases the Weapon Damage of your ships when they take part in an Open Armada attack on a Hostile target. | Armada Control Center |
| `buff_43199216` | The Dilithium Storage indicates how much Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator H |
| `buff_437521644` | The Tritanium Storage indicates how much Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator H |
| `buff_439527927` | The base Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator F |
| `buff_439788362` | Defense Platforms protect your Station from other players' attacks. | Defense Platform F |
| `buff_444879361` | The amount of Active Plasma that can be harvested per hour. | Active Plasma Harvester A |
| `buff_459056552` | The amount of Magnetic Plasma which can be stored within this harvester. | Magnetic Plasma Harvester C |
| `buff_473361651` | The base Piercing stats of all your ships against hostiles is increased each time you upgrade the Holodeck building. | Holodeck |
| `buff_503510825` | The Σ-Dilithium Storage indicates how much Σ-Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator G |
| `buff_509573507` | The Σ-Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator F |
| `buff_550553432` | Increases the base amount of time a ship can stay in the Mirror Universe. | District 56 |
| `buff_555608088` | The Dilithium Storage indicates how much Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator F |
| `buff_564195136` | The Σ-Dilithium Storage indicates how much Σ-Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator C |
| `buff_572203286` | The Research and Development building increases your Research speed each time you upgrade it. | R&D Department |
| `buff_615973361` | The amount of Magnetic Plasma that is protected when your Alliance Starbase is attacked. | Magnetic Plasma Vault B |
| `buff_617151024` | The Σ-Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator D |
| `buff_624435206` | The base Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator B |
| `buff_625098481` | Increases base Scrapping Speed. | Transogen Forge |
| `buff_633581023` | The Σ-Parsteel Storage indicates how much Σ-Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator C |
| `buff_636256484` | The Maximum amount of Σ-Tritanium you can stock from the Generators is increased each time you upgrade the Tritanium Warehouse. | Tritanium Warehouse |
| `buff_637758986` | Unlocks the Orion Syndicate Bomb Prototype Tech | Recon Locus |
| `buff_638568962` | Increases the Hull Health of the Alliance Starbase. | Command Control |
| `buff_639634280` | Increases Cost Efficiency of Crystal, Gas, and Ore for all Buildings | INDEPENDENT ARCHIVES |
| `buff_65043874` | The Σ-Dilithium Storage indicates how much Σ-Dilithium this Generator can hold at once. It increases each time you upgrade it. | Dilithium Generator A |
| `buff_667990520` | The Σ-Parsteel Storage indicates how much Σ-Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator B |
| `buff_673935722` | Upgrade the Signal Observatory to increase the Challenge Credits earned from the V'ger Challenge Track.  Level 20: 20%  Level 30: 30%  Level 40: 40%  Level 50: 50%  Level 60: 60%  Level 70: 90%  Level 75: 120% | Signal Observatory |
| `buff_711561428` | The Tritanium Storage indicates how much Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator B |
| `buff_727283729` | The base Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator F |
| `buff_729229730` | The base Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator D |
| `buff_746818829` | Unlocks a Transogen Exocomp Slot. | Transogen Forge |
| `buff_753685799` | The Shield Health of all your Explorers is increased each time you upgrade the Science Lab building. | Science Lab |
| `buff_764445510` | The Σ-Tritanium Storage indicates how much Σ-Tritanium this Generator can hold at once. It increases each time you upgrade it. | Tritanium Generator E |
| `buff_776081615` | Increases base Uncommon Transogen Mining rate. | Transogen Forge |
| `buff_778737357` | The Σ-Parsteel Storage indicates how much Σ-Parsteel this Generator can hold at once. It increases each time you upgrade it. | Parsteel Generator D |
| `buff_787978047` | The Repair Costs for the ship that is affiliated to this Drydock is decreased every time you upgrade it. | Drydock D |
| `buff_802394140` | The base Dilithium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Dilithium Generator E |
| `buff_848074636` | The amount of Tritanium that cannot be stolen when your station gets attacked is increased each time you upgrade the Tritanium Vault. | Tritanium Vault |
| `buff_885203147` | The Maximum amount of Dilithium you can stock from the Generators is increased each time you upgrade the Dilithium Warehouse. | Dilithium Warehouse |
| `buff_891466842` | The amount of Magnetic Plasma which can be stored within this harvester. | Magnetic Plasma Harvester B |
| `buff_90336722` | The Σ-Tritanium Production Rate Per Hour of this Generator is increased each time you upgrade it. | Tritanium Generator H |
| `buff_912296055` | The Σ-Parsteel Production Rate Per Hour of this Generator is increased each time you upgrade it. | Parsteel Generator F |
| `buff_924439393` | Increases base speed of constructing Station Modules | Recon Locus |
| `buff_92563433` | Increases the amount of Common, <color=#00ed4b>Uncommon</color> and <color=#00b4ff>Rare</color> Solo Outpost Components your ship receives from Solo Outpost Retaliation attacks. | Outpost Control Center |
| `buff_934971217` | The amount of Collisional Plasma that can be processed by the Alliance Starbase before it needs to relocate to a different Plasma Storm. It is increased each time it is upgraded. | Plasma Processor |
| `buff_979710652` | The amount of additional Cardassian Loot or Superior Cardassian Loot awarded to Alliance Members when they destroy a Cardassian Station. | Salvage Yard |

## Conditions not in `is_known_building_condition`

None.

