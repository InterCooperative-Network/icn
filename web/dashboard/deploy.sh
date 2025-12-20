#!/bin/bash
# ICN Dashboard Deployment Script

set -e

echo "🚀 ICN Node Dashboard Deployment"
echo "================================"
echo ""

# Check if a method was provided
if [ -z "$1" ]; then
    echo "Usage: ./deploy.sh [python|node|docker]"
    echo ""
    echo "Options:"
    echo "  python  - Deploy with Python HTTP server (simplest)"
    echo "  node    - Deploy with Node.js HTTP server"
    echo "  docker  - Deploy with Docker container"
    echo ""
    exit 1
fi

METHOD=$1
PORT=${2:-8080}

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
        docker build -t icn-dashboard .
        
        echo "📦 Starting Docker container on port $PORT..."
        docker run -d \
            --name icn-dashboard \
            --restart unless-stopped \
            -p $PORT:80 \
            icn-dashboard
        
        echo "✅ Dashboard deployed!"
        echo "📊 Access at: http://localhost:$PORT"
        echo ""
        echo "To stop: docker stop icn-dashboard"
        echo "To remove: docker rm icn-dashboard"
        ;;
    
    *)
        echo "❌ Invalid method: $METHOD"
        echo "Use: python, node, or docker"
        exit 1
        ;;
esac
