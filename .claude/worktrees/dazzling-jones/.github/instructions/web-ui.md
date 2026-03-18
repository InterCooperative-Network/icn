---
applyTo: "web/**/*.{js,html,css}"
---

# Web UI Instructions

These instructions apply to the web frontend applications in the `web/` directory.

## Technology Stack

- **Vanilla JavaScript** (ES6+) - No frameworks, intentionally simple
- **HTML5** - Semantic markup
- **CSS3** - Modern styling with CSS variables
- **Progressive Web App (PWA)** - Service workers, offline support
- **Testing**: Jest for unit tests, Playwright for E2E tests

## Code Style

- Use modern ES6+ features (async/await, arrow functions, destructuring)
- Prefer `const` over `let`, avoid `var`
- Use template literals for string interpolation
- Use async/await instead of raw promises
- Follow existing component patterns (see `components/` directory)

## Architecture Patterns

### Component Pattern

```javascript
// Component structure (see components/*.js)
export class ComponentName {
    constructor(container) {
        this.container = container;
        this.state = {};
    }

    async init() {
        await this.fetchData();
        this.render();
        this.attachEventListeners();
    }

    render() {
        this.container.innerHTML = `...`;
    }

    attachEventListeners() {
        // Event delegation preferred
    }
}
```

### API Client Pattern

- Use the centralized API client in `app.js`
- Always handle errors with user-friendly messages
- Show loading states during async operations
- Use toast notifications for feedback

### Error Handling

- Translate technical errors to user-friendly messages
- Use the `showError()` function for consistent error display
- Log technical details to console for debugging
- Never show raw error messages to users

Example:
```javascript
try {
    const result = await apiClient.post('/endpoint', data);
    showToast('Success!', 'success');
} catch (error) {
    const userMessage = translateError(error);
    showToast(userMessage, 'error');
    console.error('Technical details:', error);
}
```

## Testing

### Unit Tests (Jest)

- Test pure functions and utility code
- Mock API calls
- Test error handling paths
- Located in `tests/unit/`

### E2E Tests (Playwright)

- Test complete user workflows
- Use data-testid attributes for selectors
- Test both success and error scenarios
- Located in `tests/e2e/`

## PWA Conventions

- Service worker in `sw.js` handles offline caching
- `manifest.json` defines app metadata
- Icons must be in `icons/` directory (various sizes)
- Cache static assets for offline use

## Important Notes

- **No build step**: Files are served directly (intentionally simple)
- **Token management**: Use localStorage for auth tokens with expiration tracking
- **WebSocket**: Real-time updates via WebSocket connection to gateway
- **Mobile-first**: Responsive design with mobile as primary target
- **Accessibility**: Use semantic HTML and ARIA labels
- **Browser support**: Modern browsers (Chrome, Firefox, Safari, Edge)

## Common Tasks

### Adding a New Page

1. Create HTML file (e.g., `new-page.html`)
2. Create corresponding JS file (e.g., `new-page.js`)
3. Create corresponding CSS file (e.g., `new-page.css`)
4. Add navigation link to main pages
5. Register in service worker cache if needed
6. Add E2E tests

### Adding a New Component

1. Create in `components/` directory
2. Export as ES6 class or functions
3. Follow existing naming conventions
4. Add unit tests
5. Document component API

### Making API Calls

- Use the API client from `app.js`
- Handle authentication token automatically
- Use appropriate HTTP methods (GET, POST, PUT, DELETE)
- Always handle errors with user-friendly messages

## Documentation

- README files for each major feature
- Inline comments for complex logic only
- JSDoc comments for exported functions/classes
- User-facing documentation in GETTING-STARTED.md, FAQ.md, etc.
