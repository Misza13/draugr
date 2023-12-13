// Connect to Aardwolf MUD
connect("aardmud.org", 4000);

// Wait for prompt for character's name
expect("What be thy name, adventurer[?]");
// Send character name
send("Insert-name-here");

// Wait for password prompt
expect("Password:");
// Send password
send_secret("Insert-password-here");

// Wait briefly for the "Press Return to continue" prompt
sleep(0.5);
// Send an empty line to skip the prompt
send("");