#!/bin/bash
# Deploy ICN API Documentation

set -e

echo "📚 ICN API Documentation Deployment"
echo "===================================="
echo ""

METHOD=${1:-python}
PORT=${2:-3000}

case $METHOD in
    python)
        echo "📦 Starting Python HTTP server on port $PORT..."
        python3 -m http.server $PORT
        ;;
    
    node)
        echo "📦 Installing http-server (if needed)..."
        if ! command -v http-server &> /dev/null; then
            npm install -g http-server
        fi
        
        echo "📦 Starting Node.js HTTP server on port $PORT..."
        http-server -p $PORT
        ;;
    
    docker)
        echo "📦 Building Docker image..."
        docker build -t icn-api-docs .
        
        echo "📦 Starting Docker container on port $PORT..."
        docker run -d \
            --name icn-api-docs \
            --restart unless-stopped \
            -p $PORT:80 \
            icn-api-docs
        
        echo "✅ API docs deployed!"
        echo "📚 Access at: http://localhost:$PORT"
        echo ""
        echo "To stop: docker stop icn-api-docs"
        echo "To remove: docker rm icn-api-docs"
        ;;
    
    *)
        echo "Usage: ./deploy.sh [python|node|docker] [port]"
        echo ""
        echo "Examples:"
        echo "  ./deploy.sh python 3000"
        echo "  ./deploy.sh node 8080"
        echo "  ./deploy.sh docker 80"
        exit 1
        ;;
esac
