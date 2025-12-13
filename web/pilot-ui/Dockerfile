# Static PWA - Nginx serving
FROM nginx:alpine

# Copy static files
COPY index.html /usr/share/nginx/html/
COPY app.js /usr/share/nginx/html/
COPY style.css /usr/share/nginx/html/
COPY sw.js /usr/share/nginx/html/
COPY manifest.json /usr/share/nginx/html/
COPY offline.html /usr/share/nginx/html/

# SDIS pages
COPY sdis-*.html /usr/share/nginx/html/
COPY sdis-*.css /usr/share/nginx/html/
COPY sdis-*.js /usr/share/nginx/html/

# Steward dashboard
COPY steward-dashboard.html /usr/share/nginx/html/
COPY steward-dashboard.css /usr/share/nginx/html/
COPY steward-dashboard.js /usr/share/nginx/html/

# Add custom nginx config for SPA routing with no-cache for JS/CSS
RUN echo 'server { \
    listen 3000; \
    root /usr/share/nginx/html; \
    index index.html; \
    location / { \
        try_files $uri $uri/ /index.html; \
    } \
    # Disable caching for JS and CSS files during development \
    location ~* \.(js|css)$ { \
        add_header Cache-Control "no-cache, no-store, must-revalidate"; \
        add_header Pragma "no-cache"; \
        add_header Expires "0"; \
    } \
    # Security headers \
    add_header X-Frame-Options "SAMEORIGIN" always; \
    add_header X-Content-Type-Options "nosniff" always; \
    add_header X-XSS-Protection "1; mode=block" always; \
}' > /etc/nginx/conf.d/default.conf

EXPOSE 3000

CMD ["nginx", "-g", "daemon off;"]
