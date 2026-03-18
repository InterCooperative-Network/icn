# PWA Icon Generation Guide

The ICN Timebank PWA requires icons in multiple sizes for different platforms and contexts.

## Required Icon Sizes

Create icons in the following sizes:

- **72x72** - Android legacy
- **96x96** - Android standard
- **128x128** - Chrome Web Store
- **144x144** - Microsoft tiles
- **152x152** - iOS legacy
- **192x192** - Android baseline (required)
- **384x384** - Android high-res
- **512x512** - Android extra high-res (required)

## Icon Design Guidelines

### Visual Design

1. **Simple & Recognizable**
   - Use a clear, simple design that works at small sizes
   - Avoid fine details that disappear at 72x72
   - High contrast against both light and dark backgrounds

2. **Suggested Design**
   - Central element: Clock or hourglass (represents time)
   - Secondary element: Handshake or people (represents cooperation)
   - Color scheme: ICN brand colors (blue #2563eb primary)

3. **Maskable Icons**
   - Safe zone: Center 80% of canvas
   - Allow 10% padding on all sides for platform-specific masking
   - Background should extend to edges

### Technical Requirements

- **Format**: PNG (with transparency for non-maskable)
- **Color**: RGB color space
- **Background**: Transparent for regular icons, solid color for maskable
- **File size**: Optimize for web (aim for <50KB per icon)

## Quick Generation with Online Tools

### Option A: PWA Builder Icon Generator
1. Visit: https://www.pwabuilder.com/imageGenerator
2. Upload a 512x512 source image
3. Download generated icon pack
4. Extract to `web/pilot-ui/icons/`

### Option B: Favicon.io
1. Visit: https://favicon.io/favicon-converter/
2. Upload a square logo (at least 512x512)
3. Download and extract
4. Rename files to match manifest.json requirements

### Option C: ImageMagick (Command Line)

If you have a 1024x1024 source image (`source.png`):

```bash
# Create icons directory
mkdir -p icons

# Generate all required sizes
for size in 72 96 128 144 152 192 384 512; do
    convert source.png -resize ${size}x${size} icons/icon-${size}x${size}.png
done
```

## Icon Filenames

Place icons in `web/pilot-ui/icons/` with these exact names:

```
icons/
├── icon-72x72.png
├── icon-96x96.png
├── icon-128x128.png
├── icon-144x144.png
├── icon-152x152.png
├── icon-192x192.png
├── icon-384x384.png
└── icon-512x512.png
```

## Shortcut Icons (Optional)

For app shortcuts (manifest.json shortcuts):

```
icons/
├── shortcut-log.png     (96x96) - Clock/plus icon for "Log Hours"
├── shortcut-history.png (96x96) - List icon for "History"
└── shortcut-vote.png    (96x96) - Ballot box for "Governance"
```

## Screenshots (Optional, for App Stores)

Create screenshots for better app listing:

- **Desktop**: 1280x720 (wide screenshot)
- **Mobile**: 750x1334 (iPhone 8 size)

Place in `web/pilot-ui/screenshots/`:

```
screenshots/
├── dashboard.png           (1280x720) - Desktop view
└── mobile-dashboard.png    (750x1334) - Mobile view
```

## Badge Icons (for Notifications)

For push notifications badge (monochrome):

```
icons/
└── badge-72x72.png  (72x72, white silhouette on transparent)
```

## Testing Icons

### Chrome DevTools

1. Open Chrome DevTools (F12)
2. Go to Application tab
3. Click "Manifest" in sidebar
4. Verify all icons load correctly

### Lighthouse Audit

1. Run Lighthouse audit (DevTools > Lighthouse)
2. Check "Progressive Web App" category
3. Ensure "Installable" passes
4. Fix any icon-related warnings

## Placeholder Icons (Temporary)

If you need placeholder icons temporarily:

1. Create a simple colored square with text:
   ```html
   <svg width="512" height="512">
     <rect width="512" height="512" fill="#2563eb"/>
     <text x="256" y="256" text-anchor="middle" fill="white" font-size="200">ICN</text>
   </svg>
   ```

2. Save as SVG and convert to PNG

3. Use online tool: https://svgtopng.com/

## Icon Checklist

Before deploying:

- [ ] All 8 required sizes created (72, 96, 128, 144, 152, 192, 384, 512)
- [ ] Icons follow maskable safe zone (80% center)
- [ ] Icons optimized for file size (<50KB each)
- [ ] Icons placed in `web/pilot-ui/icons/` directory
- [ ] Manifest.json paths match actual icon files
- [ ] Icons tested in Chrome DevTools Application tab
- [ ] PWA passes Lighthouse "Installable" check
- [ ] Icons look good on both light and dark backgrounds
- [ ] Optional: Shortcut icons created
- [ ] Optional: Screenshots created for app stores

## Recommended Source Image

Create or commission a **1024x1024** source image with:

- ICN branding
- Time/cooperation theme
- Works at small sizes (readable at 72x72)
- Transparent background (or solid for maskable variant)

Then use tools above to generate all required sizes.

---

**Note**: The app will work without icons, but proper icons significantly improve the user experience and installation flow.
