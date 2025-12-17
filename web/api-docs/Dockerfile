FROM nginx:alpine

# Copy API documentation files
COPY index.html /usr/share/nginx/html/
COPY openapi.yaml /usr/share/nginx/html/
COPY README.md /usr/share/nginx/html/

# Expose port 80
EXPOSE 80

# Start nginx
CMD ["nginx", "-g", "daemon off;"]
