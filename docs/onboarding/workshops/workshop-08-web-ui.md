# Workshop 8: Web UI Exploration

## Goal
Understand how the Pilot UI connects to the ICN gateway and trace user actions
through the stack.

## Prerequisites
- Completed Module 8 reading
- Node.js installed (for running the UI locally)

## Estimated time
1-2 hours

## Part 1: UI Project Structure

### Steps
1. Navigate to the web UI directory:
   ```bash
   cd web/pilot-ui
   ```

2. List the project structure:
   ```bash
   ls -la
   ```

3. Identify the main files:
   - `index.html` - Entry point and layout
   - `app.js` - Core application logic
   - `components/` - Feature modules

### Questions to answer
1. Is this a single-page application (SPA)?
2. What framework/library is used for the UI?
3. Where is the routing handled?

### Checkpoint
- [ ] You identified the main entry point
- [ ] You understand the project structure

## Part 2: Login Flow Analysis

### Steps
1. Open `web/pilot-ui/app.js`
2. Find the `login` function
3. Trace the login sequence:
   - Input validation
   - Health check
   - Auth verification
   - Session persistence

### Expected flow
```
User Input → Validate Fields → GET /health → GET /ledger/.../balance → Store in localStorage
```

### Code to examine
Look for localStorage operations:
```javascript
localStorage.setItem('icn-gateway', state.gatewayUrl);
localStorage.setItem('icn-token', state.token);
```

### Questions to answer
1. What happens if fields are missing?
2. What endpoint is called to verify auth works?
3. How long is the token assumed valid?

### Checkpoint
- [ ] You traced the complete login flow
- [ ] You found the session persistence code

## Part 3: API Request Pattern

### Steps
1. Find the `apiRequest` function in `app.js`
2. Examine how the JWT is attached to requests
3. Identify error handling patterns

### Expected pattern
```javascript
async function apiRequest(method, path, body) {
    const response = await fetch(state.gatewayUrl + path, {
        method,
        headers: {
            'Authorization': `Bearer ${state.token}`,
            'Content-Type': 'application/json'
        },
        body: body ? JSON.stringify(body) : undefined
    });
    // Error handling...
}
```

### Checkpoint
- [ ] You understand how the JWT is used
- [ ] You found the error handling logic

## Part 4: Map UI Actions to API Endpoints

### Exercise
Fill in this table by examining the UI code:

| UI Action | HTTP Method | API Endpoint |
|-----------|------------|--------------|
| View balance | GET | /ledger/{coopId}/balance/{did} |
| Send transfer | ? | ? |
| List members | ? | ? |
| View proposals | ? | ? |
| Vote on proposal | ? | ? |

### Steps
1. Search for fetch calls in `app.js` and `components/`
2. Map each UI feature to its API call
3. Note the HTTP method and path pattern

### Checkpoint
- [ ] You mapped at least 3 UI actions to endpoints
- [ ] You understand the REST API pattern

## Part 5: WebSocket Events

### Steps
1. Search for WebSocket usage:
   ```bash
   grep -r "WebSocket" web/pilot-ui/ --include="*.js"
   ```

2. Find where the WebSocket connection is established
3. Identify what events are handled

### Questions to answer
1. What URL pattern is used for WebSocket connections?
2. What events does the UI listen for?
3. How does the UI handle reconnection?

### Checkpoint
- [ ] You found the WebSocket connection code
- [ ] You understand what real-time events are handled

## Part 6: Run the UI Locally (Optional)

### Prerequisites
- A running ICN gateway (or mock server)
- Valid JWT token

### Steps
1. Install dependencies:
   ```bash
   cd web/pilot-ui
   npm install
   ```

2. Start the development server:
   ```bash
   npm run dev
   ```

3. Open the browser and navigate to the local URL

4. Try connecting with:
   - Gateway URL: `http://localhost:8080` (or your gateway)
   - Coop ID: Your test cooperative
   - DID: Your test identity
   - JWT: A valid token

### Troubleshooting
If you don't have a running gateway, examine the UI behavior with invalid
credentials to understand the error handling.

### Checkpoint
- [ ] You ran the UI locally (or understand why you couldn't)
- [ ] You traced a real or simulated request flow

## Summary
After completing this workshop you should be able to:
- Navigate the Pilot UI codebase
- Trace the login and session flow
- Map UI actions to gateway API calls
- Understand WebSocket event handling

## Next steps
Proceed to Module 9: Operations and Deployment
