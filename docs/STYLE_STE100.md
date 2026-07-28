# Documentation style guide (ASD-STE100)

This guide tells you how to write the user-facing documents of KOBAYASHI. It applies
ASD-STE100 Simplified Technical English (STE), Issue 9. STE keeps technical text short,
clear, and easy to translate.

## 1. Documents in scope

These documents follow STE:

- [README.md](../README.md)
- [CONTRIBUTING.md](../CONTRIBUTING.md)
- [docs/README.md](README.md)
- [docs/SYNC.md](SYNC.md)
- [docs/DEPLOYMENT_SECURITY.md](DEPLOYMENT_SECURITY.md)
- [docs/LCARS_CONTRIBUTING.md](LCARS_CONTRIBUTING.md)
- [docs/CALIBRATION_ADD_FIGHT.md](CALIBRATION_ADD_FIGHT.md)

These items do not follow STE:

- Code blocks, command examples, log output, and configuration samples.
- YAML keys, JSON keys, environment variable names, and API paths.
- Generated documents, for example [CALIBRATION_SCOREBOARD.md](CALIBRATION_SCOREBOARD.md),
  [OFFICER_MODELING_SCORECARD.md](OFFICER_MODELING_SCORECARD.md), and the files in
  [audit_shards/](audit_shards/).
- Quotations, product names, and the project name.
- Design documents and research notes in `docs/` that are not in the list above.

## 2. Writing rules

### 2.1 Sentences

1. Write not more than 20 words in an instruction. Write not more than 25 words in a
   descriptive sentence.
2. Write only one instruction in a sentence. If a step has two actions, write two steps.
3. Write not more than 6 sentences in a paragraph of procedural text.
4. Do not use a sentence fragment when a full sentence is possible. A list item can be a
   noun phrase.

### 2.2 Verbs

5. Use the active voice. Write "The server writes the file", not "The file is written by
   the server".
6. Use the imperative in instructions. Write "Extract the archive", not "The archive
   should be extracted".
7. Use simple present, simple past, or simple future. Do not use the progressive form
   ("is running") or the perfect form ("has run").
8. Do not use the `-ing` form as a noun. Write "Sync the roster" or "the sync operation",
   not "Syncing the roster".
9. Use "must" for a requirement. Use "can" for a possibility or an ability. Do not use
   "should", "shall", or "may".

### 2.3 Words

10. Use one word for one meaning, and one meaning for one word. See the replacement table
    in section 4.
11. Use the same term for the same thing in all documents. See the technical names in
    section 3.
12. Keep the articles "a", "an", and "the". Do not remove words to make the text shorter.
13. Do not use a noun cluster of more than three words. Write "the token for the profile
    sync", not "the profile sync token secret".
14. Do not use a slash to show a choice. Write "the tier or the level", not "tier/level".
15. Do not use "and/or".

### 2.4 Structure

16. Give each paragraph one topic. Put the topic in the first sentence.
17. Use a numbered list for a sequence of steps. Use a bullet list for items with no
    sequence.
18. Put a warning or a caution before the step that it applies to. Start it with a command.
19. Do not use humor, metaphors, or idioms in technical text.

## 3. Approved technical names and technical verbs

STE lets a project add technical names and technical verbs for its own domain. These are
the approved words for KOBAYASHI. Use them with these meanings only.

### 3.1 Technical names

| Term | Meaning |
| --- | --- |
| ability | A named effect of an officer, a ship, or a hostile. |
| below decks | The crew slots that are not the captain slot and not the bridge slots. |
| bridge | The two crew slots next to the captain slot. |
| buff | A bonus that changes a statistic in combat. |
| captain | The crew slot that gives the captain ability. |
| crew | One captain, two bridge officers, and the below decks officers. |
| fight | One simulated combat between a ship and a hostile. |
| forbidden tech | A player upgrade that gives combat bonuses. |
| hostile | A computer-controlled target ship in the game. |
| hull | The health of a ship after the shield is at zero. |
| LCARS | Language for Combat Ability Resolution and Simulation, the YAML format for officer abilities. |
| mitigation | The reduction of damage by the shield of the target. |
| officer | A crew member with up to three abilities. |
| optimizer | The component that searches the crew space. |
| profile | The set of player bonuses: research, buildings, reputation, artifacts, exocomps, and forbidden tech. |
| proc | A random event in a fight that starts an effect. |
| roster | The list of officers that a player owns. |
| round | One cycle of the fight loop. |
| seed | The number that sets the start state of the random number generator. |
| ship | A player vessel with a tier and a level. |
| simulation | A set of fights with the same crew and the same conditions. |
| sync | The transfer of game state from the STFC Community Mod to KOBAYASHI. |
| tier | The upgrade stage of a ship or of forbidden tech. |

### 3.2 Technical verbs

| Verb | Meaning |
| --- | --- |
| to build | To compile the source code into a binary. |
| to bind | To attach the server to a network address. |
| to import | To read player data from a file into a profile. |
| to merge | To add new data to existing data in the same file. |
| to optimize | To search the crew space for the best crew. |
| to parse | To read a text format into a data structure. |
| to simulate | To calculate the result of a fight. |
| to sync | To send game state from the mod to the server. |
| to validate | To check data against the schema and the rules. |

## 4. Replacement table

Do not use the word in the first column. Use the word in the second column.

| Do not use | Use |
| --- | --- |
| additional | more, other |
| allow | let, make it possible |
| approximately | about |
| attempt | try |
| currently | now |
| deep dive, at a glance | (delete, or write a plain sentence) |
| e.g. | for example |
| ensure | make sure that |
| etc. | (list the items, or write "and other …") |
| execute | run, do |
| however | but |
| i.e. | that is |
| in addition | also |
| in order to | to |
| indicate | show |
| initiate | start |
| leverage, utilize | use |
| obtain | get |
| occur | happen |
| perform | do |
| prior to | before |
| provide | give, supply |
| regarding | about |
| require | must have, need |
| retain | keep |
| such as | for example |
| terminate | stop |
| typically | in most conditions |
| verify | check, make sure that |
| via | with, by |

## 5. Checks before you commit

1. Read each sentence aloud. If you cannot say it in one breath, make it shorter.
2. Count the words in each instruction. The limit is 20.
3. Find each `-ing` word. Change it to a simple verb or to a noun.
4. Find each passive verb. Name the actor and use the active voice.
5. Find each word in the first column of the table in section 4.
6. Check that the terms agree with section 3.
