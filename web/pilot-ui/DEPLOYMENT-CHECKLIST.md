# Deployment Checklist - Phase 3 Release

Complete checklist for deploying Phase 3 improvements to production.

## Pre-Deployment

### 1. Review Changes

- [ ] Read [PHASE3-IMPROVEMENTS.md](PHASE3-IMPROVEMENTS.md) for complete feature list
- [ ] Review [README.md](README.md) for updated documentation links
- [ ] Check that all new files are present:
  - [ ] `QUICK-START.md` (467 lines)
  - [ ] `TREASURER-GUIDE.md` (584 lines)
  - [ ] `ADMIN-GUIDE.md` (738 lines)
  - [ ] `FAQ.md` (560 lines)
  - [ ] `PHASE3-IMPROVEMENTS.md` (420 lines)

### 2. Backup Current Version

```bash
# Create backup of current deployment
cd /var/www/icn-pilot-ui
tar -czf ../icn-pilot-ui-backup-$(date +%Y%m%d-%H%M%S).tar.gz .

# Verify backup
ls -lh ../icn-pilot-ui-backup-*.tar.gz
```

### 3. Test in Staging (if available)

- [ ] Deploy to staging environment
- [ ] Test all keyboard shortcuts (Ctrl+1-5)
- [ ] Test copy DID button functionality
- [ ] Test transaction sorting dropdown
- [ ] Verify balance chart renders correctly
- [ ] Check proposals widget displays
- [ ] Verify top contributors leaderboard
- [ ] Test on mobile devices (iOS and Android)
- [ ] Test on all browsers (Chrome, Firefox, Safari, Edge)

## Deployment Steps

### 1. Upload Files

```bash
# Navigate to deployment directory
cd /var/www/icn-pilot-ui

# Upload main files (replace existing)
# Option A: Via SCP
scp index.html user@server:/var/www/icn-pilot-ui/
scp app.js user@server:/var/www/icn-pilot-ui/
scp style.css user@server:/var/www/icn-pilot-ui/

# Option B: Via Git
git pull origin main

# Upload new documentation files
scp QUICK-START.md user@server:/var/www/icn-pilot-ui/
scp TREASURER-GUIDE.md user@server:/var/www/icn-pilot-ui/
scp ADMIN-GUIDE.md user@server:/var/www/icn-pilot-ui/
scp FAQ.md user@server:/var/www/icn-pilot-ui/
scp PHASE3-IMPROVEMENTS.md user@server:/var/www/icn-pilot-ui/
```

### 2. Verify File Permissions

```bash
# Ensure web server can read files
chmod 644 *.html *.js *.css *.md
chown www-data:www-data *.html *.js *.css *.md
```

### 3. Clear Caches

```bash
# Option A: Add cache-busting query parameter
# Update index.html to reference:
# <link rel="stylesheet" href="style.css?v=3.0.0">
# <script src="app.js?v=3.0.0"></script>

# Option B: Clear reverse proxy cache (if using nginx)
nginx -s reload

# Option C: Clear CDN cache (if applicable)
# Follow your CDN provider's instructions
```

### 4. Test Production Deployment

- [ ] Access the UI in a browser
- [ ] **Hard refresh** (Ctrl+Shift+R or Cmd+Shift+R) to clear cache
- [ ] Sign in successfully
- [ ] Test keyboard shortcuts work
- [ ] Test copy DID button
- [ ] Verify balance chart displays
- [ ] Check all tabs load correctly
- [ ] Test on mobile device

## Post-Deployment

### 1. Verify New Features

#### Quick Wins
- [ ] Press Ctrl+1 → Dashboard loads
- [ ] Press Ctrl+2 → Log Hours loads
- [ ] Press Ctrl+3 → History loads
- [ ] Press Ctrl+4 → Members loads
- [ ] Press Ctrl+5 → Governance loads
- [ ] Click 📋 next to a member DID → "DID copied" toast appears
- [ ] Transaction sort dropdown has 4 options
- [ ] Balance shows trend arrow (↑↓→)
- [ ] Open proposals show deadline countdown

#### Dashboard Enhancements
- [ ] Balance chart renders (if there are transactions in last 30 days)
- [ ] "Pending Proposals" widget shows open proposals
- [ ] "Top Contributors" shows leaderboard with medals
- [ ] "Vote Now" button navigates to Governance tab

#### Documentation
- [ ] Navigate to `/QUICK-START.md` → Opens guide
- [ ] Navigate to `/TREASURER-GUIDE.md` → Opens guide
- [ ] Navigate to `/ADMIN-GUIDE.md` → Opens guide
- [ ] Navigate to `/FAQ.md` → Opens guide

### 2. Monitor for Issues

```bash
# Check web server error logs
tail -f /var/log/nginx/error.log

# Check browser console for JavaScript errors
# Open Developer Tools (F12) → Console tab
# Look for errors (red text)
```

### 3. Performance Check

- [ ] Page load time < 2 seconds
- [ ] Balance chart renders in < 500ms
- [ ] No console errors or warnings
- [ ] Mobile scrolling is smooth
- [ ] No layout shift after page load

### 4. Cross-Browser Testing

Test on:
- [ ] Chrome (desktop)
- [ ] Chrome (mobile)
- [ ] Firefox (desktop)
- [ ] Safari (desktop)
- [ ] Safari (iOS)
- [ ] Edge (desktop)

Look for:
- [ ] Layout looks correct
- [ ] All buttons work
- [ ] Charts render properly
- [ ] No visual glitches

## User Communication

### 1. Announce New Features

**Email Template**:

```
Subject: 🎉 New Timebank Features Available Now!

Hi everyone,

We've just released a major update with lots of new features to make the timebank easier to use:

**Quick Wins**:
- ⌨️ Keyboard shortcuts (Ctrl+1-5) for fast navigation
- 📋 One-click DID copying
- 🔄 Sort transactions by date or amount
- 📈 Balance trend indicator (see if you're up, down, or stable)
- ⏰ Proposal deadline countdown (never miss a vote!)

**Dashboard Improvements**:
- 📊 Balance chart showing your activity over time
- 🗳️ Pending proposals widget
- 🏆 Top contributors leaderboard

**New Guides**:
- 📖 Quick Start Guide (5-minute onboarding)
- 💰 Treasurer's Guide (financial management)
- ⚙️ Admin Guide (system administration)
- ❓ FAQ (common questions answered)

Check out the new features: [Your Timebank URL]

Questions? Check the FAQ or reply to this email.

Thanks,
[Your Name]
```

### 2. Update Documentation Links

Update any existing member materials to link to new guides:
- Onboarding emails → Link to QUICK-START.md
- Treasurer handbook → Link to TREASURER-GUIDE.md
- Admin documentation → Link to ADMIN-GUIDE.md

### 3. Provide Training (Optional)

Consider hosting a:
- [ ] Quick demo video (5 minutes)
- [ ] Live walkthrough meeting (30 minutes)
- [ ] Q&A session for treasurers/admins

## Rollback Plan

If issues arise, rollback to previous version:

```bash
# Stop web server
systemctl stop nginx

# Remove new files
cd /var/www/icn-pilot-ui
rm -f index.html app.js style.css

# Restore from backup
tar -xzf ../icn-pilot-ui-backup-[timestamp].tar.gz

# Restart web server
systemctl start nginx

# Verify rollback worked
curl http://localhost:3000/
```

## Troubleshooting

### Issue: Keyboard shortcuts don't work

**Possible causes**:
- Browser cached old JavaScript
- Browser extension interfering

**Solutions**:
1. Hard refresh (Ctrl+Shift+R)
2. Clear browser cache completely
3. Try in incognito/private mode
4. Disable browser extensions

### Issue: Balance chart not displaying

**Possible causes**:
- No transactions in last 30 days
- Canvas not supported (old browser)

**Solutions**:
1. Check for transactions: History tab → "All Time"
2. Check browser version (must be Chrome 90+, Firefox 88+, etc.)
3. Check browser console for errors

### Issue: Copy DID button doesn't work

**Possible causes**:
- Clipboard API not supported (requires HTTPS or localhost)
- Browser permissions denied

**Solutions**:
1. Ensure site is served over HTTPS (not HTTP)
2. Check browser console for permission errors
3. Grant clipboard permissions in browser settings

### Issue: Documentation files return 404

**Possible causes**:
- Files not uploaded
- Wrong file path
- Web server configuration issue

**Solutions**:
1. Verify files exist: `ls -la /var/www/icn-pilot-ui/*.md`
2. Check file permissions: `chmod 644 *.md`
3. Check nginx config allows serving .md files

### Issue: Mobile layout broken

**Possible causes**:
- CSS file not updated
- Browser cache issue

**Solutions**:
1. Clear mobile browser cache
2. Verify style.css has latest version
3. Check responsive breakpoints work: resize browser window

## Monitoring

### Week 1 Post-Deployment

- [ ] Check error logs daily
- [ ] Monitor user feedback channels
- [ ] Track feature usage (if analytics available)
- [ ] Address any reported bugs within 24 hours

### Week 2-4 Post-Deployment

- [ ] Review user feedback summary
- [ ] Identify most-used features
- [ ] Identify unused features
- [ ] Plan improvements based on feedback

### Metrics to Track

- Page load time
- JavaScript errors (browser console)
- User engagement (logins, transactions logged)
- Support requests (categorize by topic)
- Documentation access (if tracking available)

## Success Criteria

Phase 3 deployment is successful when:
- [ ] All 12 new features work as documented
- [ ] No increase in support requests
- [ ] User feedback is positive (>80% satisfaction)
- [ ] Performance remains acceptable (<2s page load)
- [ ] Mobile experience is smooth
- [ ] Documentation is accessible and helpful

## Phase 4 Planning (Future)

After 4-6 weeks of Phase 3 running in production:
1. Gather user feedback
2. Review feature usage metrics
3. Identify pain points
4. Prioritize Phase 4 improvements

**Potential Phase 4 features** (based on user feedback):
- Email notifications
- Service request board
- Member profiles with skills
- Advanced reporting
- Calendar integration
- Mobile app (PWA)

---

**Deployment Date**: _________________

**Deployed By**: _________________

**Rollback Plan Tested**: [ ] Yes [ ] No

**All Checks Passed**: [ ] Yes [ ] No

**Notes**:
_________________________________________________________________
_________________________________________________________________
_________________________________________________________________

---

**Ready to deploy?** Complete this checklist and keep it for your records!
