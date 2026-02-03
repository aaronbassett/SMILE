# Building a Simple CLI Counter

This tutorial walks you through building a command-line counter application.

## Prerequisites

- Basic command-line familiarity
- A text editor

## Step 1: Create the Project

Create a new directory for your project:

```bash
mkdir my-counter && cd my-counter
```

## Step 2: Initialize the Project

Initialize the project with npm:

```bash
npm init -y
```

This creates a `package.json` file with default values.

## Step 3: Create the Counter Script

Create a file called `counter.js` with the following content:

```javascript
#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

const COUNTER_FILE = path.join(process.env.HOME, '.counter');

function readCounter() {
    try {
        return parseInt(fs.readFileSync(COUNTER_FILE, 'utf8'), 10) || 0;
    } catch {
        return 0;
    }
}

function writeCounter(value) {
    fs.writeFileSync(COUNTER_FILE, value.toString());
}

const command = process.argv[2];

switch (command) {
    case 'increment':
    case 'inc':
        const newVal = readCounter() + 1;
        writeCounter(newVal);
        console.log(`Counter: ${newVal}`);
        break;
    case 'decrement':
    case 'dec':
        const decVal = Math.max(0, readCounter() - 1);
        writeCounter(decVal);
        console.log(`Counter: ${decVal}`);
        break;
    case 'reset':
        writeCounter(0);
        console.log('Counter reset to 0');
        break;
    case 'show':
        console.log(`Counter: ${readCounter()}`);
        break;
    default:
        console.log('Usage: counter <increment|decrement|reset|show>');
}
```

## Step 4: Configure the Executable

Update the configuration in your project to make it executable. Add the appropriate settings to your configuration file.

## Step 5: Test Your Counter

Now test the counter:

```bash
./counter.js show
./counter.js increment
./counter.js show
```

You should see the counter incrementing.

## Step 6: Install Globally

Install your counter globally:

```bash
npm link
```

Now you can run it from anywhere:

```bash
counter show
```

## Conclusion

You've built a simple CLI counter! Try extending it with:
- A `set <value>` command
- Colored output
- Multiple named counters
