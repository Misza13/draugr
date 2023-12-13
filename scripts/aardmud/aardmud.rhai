/*
 * Create a file in the current directory called "env.rhai" with the following contents:
 * ```
 * export const CHARACTER_NAME = "Your-character-name";
 * export const PASSWORD = "Your-password";
 * ```
 */
import "env.rhai" as env;

// Connect to Aardwolf MUD
connect("aardmud.org", 4000);

// Wait for prompt for character's name
expect("What be thy name, adventurer[?]");
// Send character name
send(env::CHARACTER_NAME);

// Wait for password prompt
expect("Password:");
// Send password
send_secret(env::PASSWORD);

// Wait briefly for the "Press Return to continue" prompt
sleep(0.5);
// Send an empty line to skip the prompt
send("");