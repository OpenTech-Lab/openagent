#!/bin/bash
# ==============================================================================
# OpenAgent Docker Setup Script
# ==============================================================================
# Quick start script to build and run OpenAgent with Docker
# Supports multiple isolated agent environments
#
# Usage:
#   ./docker-setup.sh [agent-name]           # Full setup with onboarding
#   ./docker-setup.sh [agent-name] --build   # Build only, no onboarding
#   ./docker-setup.sh [agent-name] --start   # Start services only
#   ./docker-setup.sh [agent-name] --stop    # Stop all services
#   ./docker-setup.sh [agent-name] --clean   # Stop and remove all data
#   ./docker-setup.sh --list                 # List all agents
#
# Examples:
#   ./docker-setup.sh alice                  # Create/setup agent "alice"
#   ./docker-setup.sh bob --start            # Start agent "bob"
#   ./docker-setup.sh alice --cli chat       # Run chat for "alice"
# ==============================================================================

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color

# Default agent name
AGENT_NAME=""
AGENTS_DIR=".agents"

# Print banner
print_banner() {
    echo -e "${CYAN}"
    echo "╔═══════════════════════════════════════════════════════════════╗"
    echo "║                                                               ║"
    echo "║   ██████╗ ██████╗ ███████╗███╗   ██╗ █████╗  ██████╗ ███████╗ ║"
    echo "║  ██╔═══██╗██╔══██╗██╔════╝████╗  ██║██╔══██╗██╔════╝ ██╔════╝ ║"
    echo "║  ██║   ██║██████╔╝█████╗  ██╔██╗ ██║███████║██║  ███╗█████╗   ║"
    echo "║  ██║   ██║██╔═══╝ ██╔══╝  ██║╚██╗██║██╔══██║██║   ██║██╔══╝   ║"
    echo "║  ╚██████╔╝██║     ███████╗██║ ╚████║██║  ██║╚██████╔╝███████╗ ║"
    echo "║   ╚═════╝ ╚═╝     ╚══════╝╚═╝  ╚═══╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝ ║"
    echo "║                                                               ║"
    echo "║           High-Performance AI Agent Framework                 ║"
    echo "║                   Docker Setup Script                         ║"
    echo "╚═══════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
    if [ -n "$AGENT_NAME" ]; then
        echo -e "   ${MAGENTA}Agent: ${AGENT_NAME}${NC}"
        echo ""
    fi
}

# Print step message
step() {
    echo -e "${BLUE}==>${NC} ${GREEN}$1${NC}"
}

# Print info message
info() {
    echo -e "${CYAN}   ℹ${NC} $1"
}

# Print warning message
warn() {
    echo -e "${YELLOW}   ⚠${NC} $1"
}

# Print error message
error() {
    echo -e "${RED}   ✗${NC} $1"
}

# Print success message
success() {
    echo -e "${GREEN}   ✓${NC} $1"
}

# Validate agent name
validate_agent_name() {
    local name="$1"
    if [[ ! "$name" =~ ^[a-zA-Z][a-zA-Z0-9_-]*$ ]]; then
        error "Invalid agent name: '$name'"
        echo "   Agent name must start with a letter and contain only letters, numbers, hyphens, and underscores."
        exit 1
    fi
    if [ ${#name} -gt 32 ]; then
        error "Agent name too long (max 32 characters)"
        exit 1
    fi
}

# Get agent directory
get_agent_dir() {
    echo "${AGENTS_DIR}/${AGENT_NAME}"
}

# Get compose file path for agent
get_compose_file() {
    if [ -n "$AGENT_NAME" ]; then
        echo "$(get_agent_dir)/docker-compose.yml"
    else
        echo "docker-compose.yml"
    fi
}

# Generate unique ports for agent (based on hash of name)
generate_ports() {
    local name="$1"
    # Generate a hash-based offset (0-999) from the agent name
    local hash=$(echo -n "$name" | md5sum | cut -c1-4)
    local offset=$((16#$hash % 1000))
    
    POSTGRES_PORT=$((5432 + offset))
    GATEWAY_PORT=$((8080 + offset))

    # Check if ports are in valid range and adjust if needed
    if [ $POSTGRES_PORT -gt 65000 ]; then POSTGRES_PORT=$((5432 + (offset % 100))); fi
    if [ $GATEWAY_PORT -gt 65000 ]; then GATEWAY_PORT=$((8080 + (offset % 100))); fi
}

# Initialize agent directory and config
init_agent() {
    local agent_dir=$(get_agent_dir)
    
    step "Initializing agent '${AGENT_NAME}'..."
    
    # Create agent directory structure
    mkdir -p "${agent_dir}/workspace"
    mkdir -p "${agent_dir}/data"
    
    # Generate ports for this agent
    generate_ports "$AGENT_NAME"
    
    # Create agent-specific .env if not exists
    if [ ! -f "${agent_dir}/.env" ]; then
        if [ -f .env ]; then
            cp .env "${agent_dir}/.env"
            success "Copied .env to agent directory"
        elif [ -f .env.example ]; then
            cp .env.example "${agent_dir}/.env"
            success "Created .env from .env.example"
            warn "Please edit ${agent_dir}/.env with your API keys"
        else
            touch "${agent_dir}/.env"
            warn "Created empty .env file"
        fi
    fi

    # Ensure .env file is writable (important for Docker bind mounts)
    chmod 666 "${agent_dir}/.env" 2>/dev/null || true
    
    # Create agent-specific SOUL.md if not exists
    if [ ! -f "${agent_dir}/SOUL.md" ]; then
        if [ -f SOUL.md ]; then
            cp SOUL.md "${agent_dir}/SOUL.md"
            # Add agent name to SOUL.md
            sed -i "1s/^/# Agent: ${AGENT_NAME}\n\n/" "${agent_dir}/SOUL.md" 2>/dev/null || true
            success "Created SOUL.md for agent"
        fi
    fi
    
    # Generate docker-compose.yml for this agent
    generate_compose_file
    
    success "Agent '${AGENT_NAME}' initialized at ${agent_dir}"
    info "Ports: PostgreSQL=${POSTGRES_PORT}, Gateway=${GATEWAY_PORT}"
}

# Generate docker-compose.yml for the agent
generate_compose_file() {
    local agent_dir=$(get_agent_dir)
    local compose_file="${agent_dir}/docker-compose.yml"
    
    generate_ports "$AGENT_NAME"
    
    cat > "$compose_file" << EOF
# ==============================================================================
# OpenAgent Docker Compose - Agent: ${AGENT_NAME}
# Generated automatically - do not edit manually
# ==============================================================================

services:
  # ============================================================================
  # OpenAgent TUI - Standalone interactive chat (no databases needed)
  # ============================================================================
  ${AGENT_NAME}-tui:
    build:
      context: ../..
      dockerfile: Dockerfile
      target: runtime
    container_name: openagent-${AGENT_NAME}-tui
    stdin_open: true
    tty: true
    entrypoint: ["openagent-tui"]
    volumes:
      - ./.env:/app/.env:ro
      - ./SOUL.md:/app/SOUL.md:rw
      - ./workspace:/app/workspace:rw
      - openagent-${AGENT_NAME}-model-cache:/app/.cache
    environment:
      - RUST_LOG=\${RUST_LOG:-warn,openagent=info}
      - AGENT_NAME=${AGENT_NAME}
    networks:
      - openagent-${AGENT_NAME}-network

  # ============================================================================
  # OpenAgent CLI - Interactive command-line interface
  # ============================================================================
  ${AGENT_NAME}-cli:
    build:
      context: ../..
      dockerfile: Dockerfile
      target: runtime
    container_name: openagent-${AGENT_NAME}-cli
    stdin_open: true
    tty: true
    volumes:
      - ./.env:/app/.env:rw
      - ./SOUL.md:/app/SOUL.md:rw
      - ./workspace:/app/workspace:rw
      - openagent-${AGENT_NAME}-model-cache:/app/.cache
      - /var/run/docker.sock:/var/run/docker.sock
    environment:
      - RUST_LOG=\${RUST_LOG:-info,openagent=debug}
      - AGENT_NAME=${AGENT_NAME}
      - DATABASE_URL=postgres://openagent:openagent@openagent-${AGENT_NAME}-postgres:5432/openagent
    depends_on:
      ${AGENT_NAME}-postgres:
        condition: service_healthy
    networks:
      - openagent-${AGENT_NAME}-network

  # ============================================================================
  # OpenAgent Gateway - WebSocket/HTTP API server
  # ============================================================================
  ${AGENT_NAME}-gateway:
    build:
      context: ../..
      dockerfile: Dockerfile
      target: gateway
    container_name: openagent-${AGENT_NAME}-gateway
    ports:
      - "${GATEWAY_PORT}:8080"
    volumes:
      - ./.env:/app/.env:ro
      - ./SOUL.md:/app/SOUL.md
      - ./workspace:/app/workspace
      - openagent-${AGENT_NAME}-model-cache:/app/.cache
      - /var/run/docker.sock:/var/run/docker.sock
    environment:
      - RUST_LOG=\${RUST_LOG:-info,openagent=debug}
      - AGENT_NAME=${AGENT_NAME}
      - DATABASE_URL=postgres://openagent:openagent@openagent-${AGENT_NAME}-postgres:5432/openagent
    depends_on:
      ${AGENT_NAME}-postgres:
        condition: service_healthy
    networks:
      - openagent-${AGENT_NAME}-network
    restart: unless-stopped

  # ============================================================================
  # PostgreSQL with pgvector extension
  # ============================================================================
  ${AGENT_NAME}-postgres:
    image: pgvector/pgvector:pg16
    container_name: openagent-${AGENT_NAME}-postgres
    environment:
      POSTGRES_USER: openagent
      POSTGRES_PASSWORD: openagent
      POSTGRES_DB: openagent
    volumes:
      - openagent-${AGENT_NAME}-postgres-data:/var/lib/postgresql/data
    ports:
      - "${POSTGRES_PORT}:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U openagent -d openagent"]
      interval: 5s
      timeout: 5s
      retries: 5
    networks:
      - openagent-${AGENT_NAME}-network
    restart: unless-stopped

# ==============================================================================
# Networks
# ==============================================================================
networks:
  openagent-${AGENT_NAME}-network:
    driver: bridge

# ==============================================================================
# Volumes
# ==============================================================================
volumes:
  openagent-${AGENT_NAME}-postgres-data:
  openagent-${AGENT_NAME}-model-cache:
EOF

    success "Generated docker-compose.yml for agent '${AGENT_NAME}'"
}

# Check if Docker is running
check_docker() {
    step "Checking Docker..."
    if ! command -v docker &> /dev/null; then
        error "Docker is not installed. Please install Docker first."
        echo "   Visit: https://docs.docker.com/get-docker/"
        exit 1
    fi
    
    if ! docker info &> /dev/null; then
        error "Docker daemon is not running. Please start Docker."
        exit 1
    fi
    success "Docker is running"
}

# Check if Docker Compose is available
check_compose() {
    step "Checking Docker Compose..."
    if docker compose version &> /dev/null; then
        COMPOSE_CMD="docker compose"
        success "Docker Compose v2 found"
    elif command -v docker-compose &> /dev/null; then
        COMPOSE_CMD="docker-compose"
        success "Docker Compose v1 found"
    else
        error "Docker Compose is not installed."
        exit 1
    fi
}

# Get compose command with file
compose() {
    local compose_file=$(get_compose_file)
    if [ -n "$AGENT_NAME" ]; then
        $COMPOSE_CMD -f "$compose_file" -p "openagent-${AGENT_NAME}" "$@"
    else
        $COMPOSE_CMD "$@"
    fi
}

# Create .env file if it doesn't exist (for default agent)
setup_env() {
    if [ -n "$AGENT_NAME" ]; then
        # Agent-specific setup is handled in init_agent
        return
    fi
    
    step "Setting up environment..."
    if [ ! -f .env ]; then
        if [ -f .env.example ]; then
            cp .env.example .env
            success "Created .env from .env.example"
            warn "Please edit .env with your API keys before running onboard"
        else
            warn ".env file not found and no .env.example available"
        fi
    else
        success ".env file exists"
    fi
}

# Build Docker images
build_images() {
    step "Building Docker images (this may take a few minutes)..."
    if [ -n "$AGENT_NAME" ]; then
        compose build
    else
        $COMPOSE_CMD build
    fi
    success "Docker images built successfully"
}

# Start database services
start_databases() {
    step "Starting database services..."
    
    if [ -n "$AGENT_NAME" ]; then
        compose up -d ${AGENT_NAME}-postgres
    else
        $COMPOSE_CMD up -d openagent-postgres
    fi
    
    info "Waiting for databases to be healthy..."
    
    local pg_container="openagent-postgres"
    if [ -n "$AGENT_NAME" ]; then
        pg_container="openagent-${AGENT_NAME}-postgres"
    fi

    # Wait for PostgreSQL
    echo -n "   PostgreSQL: "
    for i in {1..30}; do
        if docker exec -t $pg_container pg_isready -U openagent -d openagent &> /dev/null; then
            echo -e "${GREEN}ready${NC}"
            break
        fi
        echo -n "."
        sleep 2
    done

    success "Database is ready"
}

# Run onboarding wizard
run_onboard() {
    step "Running OpenAgent onboarding wizard..."
    echo ""
    if [ -n "$AGENT_NAME" ]; then
        compose run --rm ${AGENT_NAME}-cli onboard
    else
        $COMPOSE_CMD run --rm openagent-cli onboard
    fi
}

# Start gateway service
start_gateway() {
    step "Starting OpenAgent gateway..."
    
    if [ -n "$AGENT_NAME" ]; then
        compose up -d ${AGENT_NAME}-gateway
        generate_ports "$AGENT_NAME"
        success "Gateway for '${AGENT_NAME}' is running on http://localhost:${GATEWAY_PORT}"
    else
        $COMPOSE_CMD up -d openagent-gateway
        success "Gateway is running on http://localhost:${GATEWAY_PORT:-8080}"
    fi
}

# Stop all services
stop_services() {
    step "Stopping services..."
    if [ -n "$AGENT_NAME" ]; then
        compose down
        success "Agent '${AGENT_NAME}' services stopped"
    else
        $COMPOSE_CMD down
        success "All services stopped"
    fi
}

# Clean up everything
clean_all() {
    step "Stopping and removing all containers, networks, and volumes..."
    if [ -n "$AGENT_NAME" ]; then
        compose down -v --remove-orphans
        local agent_dir=$(get_agent_dir)
        read -p "   Delete agent directory ${agent_dir}? [y/N] " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            rm -rf "$agent_dir"
            success "Agent '${AGENT_NAME}' completely removed"
        else
            success "Agent '${AGENT_NAME}' containers removed (directory preserved)"
        fi
    else
        $COMPOSE_CMD down -v --remove-orphans
        success "Cleanup complete"
    fi
}

# Show status
show_status() {
    step "Service Status:"
    if [ -n "$AGENT_NAME" ]; then
        compose ps
    else
        $COMPOSE_CMD ps
    fi
}

# List all agents
list_agents() {
    step "Registered Agents:"
    if [ -d "$AGENTS_DIR" ]; then
        local count=0
        for agent_dir in "$AGENTS_DIR"/*/; do
            if [ -d "$agent_dir" ]; then
                local name=$(basename "$agent_dir")
                local status="stopped"
                
                # Check if containers are running
                if docker ps --format '{{.Names}}' | grep -q "openagent-${name}-"; then
                    status="${GREEN}running${NC}"
                else
                    status="${YELLOW}stopped${NC}"
                fi
                
                # Get ports
                if [ -f "${agent_dir}/docker-compose.yml" ]; then
                    local gw_port=$(grep -oP "^\s+- \"\K\d+(?=:8080\")" "${agent_dir}/docker-compose.yml" 2>/dev/null | head -1)
                    echo -e "   • ${CYAN}${name}${NC} [${status}] - Gateway: ${gw_port:-N/A}"
                else
                    echo -e "   • ${CYAN}${name}${NC} [${status}]"
                fi
                ((count++))
            fi
        done
        if [ $count -eq 0 ]; then
            info "No agents found. Create one with: ./docker-setup.sh <agent-name>"
        fi
    else
        info "No agents found. Create one with: ./docker-setup.sh <agent-name>"
    fi
}

# Show usage
show_usage() {
    echo "Usage: $0 [agent-name] [OPTION]"
    echo ""
    echo "Create and manage multiple isolated OpenAgent instances."
    echo ""
    echo "Arguments:"
    echo "  agent-name       Name for the agent instance (letters, numbers, hyphens, underscores)"
    echo "                   If omitted, uses default single-agent mode"
    echo ""
    echo "Options:"
    echo "  (no option)      Full setup: build, start databases, run onboarding"
    echo "  --build          Build Docker images only"
    echo "  --start          Start all services (databases + gateway)"
    echo "  --stop           Stop all services"
    echo "  --clean          Stop and remove all containers and data"
    echo "  --status         Show service status"
    echo "  --cli [CMD]      Run CLI command (e.g., --cli chat)"
    echo "  --tui            Start interactive TUI chat (with tools)"
    echo "  --list           List all registered agents"
    echo "  --help           Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 alice                    # Create and setup agent 'alice'"
    echo "  $0 alice --start            # Start agent 'alice' services"
    echo "  $0 alice --cli chat         # Run chat for agent 'alice'"
    echo "  $0 alice --tui              # Interactive TUI with tools"
    echo "  $0 bob                      # Create another agent 'bob'"
    echo "  $0 --list                   # List all agents"
    echo "  $0                          # Setup default (single) agent"
    echo ""
    echo "Each agent has isolated:"
    echo "  • PostgreSQL database (separate memory/records)"
    echo "  • Workspace directory (.agents/<name>/workspace)"
    echo "  • SOUL.md personality file (.agents/<name>/SOUL.md)"
    echo "  • Environment config (.agents/<name>/.env)"
}

# Parse arguments and determine agent name
parse_args() {
    local args=()
    
    # First pass: extract agent name (first non-flag argument)
    for arg in "$@"; do
        case "$arg" in
            --*)
                args+=("$arg")
                ;;
            *)
                if [ -z "$AGENT_NAME" ] && [[ ! "$arg" =~ ^- ]]; then
                    AGENT_NAME="$arg"
                else
                    args+=("$arg")
                fi
                ;;
        esac
    done
    
    # Validate agent name if provided
    if [ -n "$AGENT_NAME" ]; then
        validate_agent_name "$AGENT_NAME"
    fi
    
    # Return remaining args
    echo "${args[@]}"
}

# Main script
main() {
    # Parse arguments - must call directly (not in subshell) to preserve AGENT_NAME
    local args=()
    for arg in "$@"; do
        case "$arg" in
            --*)
                args+=("$arg")
                ;;
            *)
                if [ -z "$AGENT_NAME" ] && [[ ! "$arg" =~ ^- ]]; then
                    AGENT_NAME="$arg"
                else
                    args+=("$arg")
                fi
                ;;
        esac
    done

    # Validate agent name if provided
    if [ -n "$AGENT_NAME" ]; then
        validate_agent_name "$AGENT_NAME"
    fi

    local remaining_args="${args[*]}"
    
    # Determine the action
    local action=""
    local cli_args=()
    local in_cli=false
    
    for arg in $remaining_args; do
        if $in_cli; then
            cli_args+=("$arg")
        else
            case "$arg" in
                --help|-h)
                    action="help"
                    ;;
                --build)
                    action="build"
                    ;;
                --start)
                    action="start"
                    ;;
                --stop)
                    action="stop"
                    ;;
                --clean)
                    action="clean"
                    ;;
                --status)
                    action="status"
                    ;;
                --list)
                    action="list"
                    ;;
                --tui)
                    action="tui"
                    ;;
                --cli)
                    action="cli"
                    in_cli=true
                    ;;
            esac
        fi
    done
    
    # Handle --list without agent name
    if [ "$action" = "list" ]; then
        print_banner
        check_docker
        list_agents
        exit 0
    fi
    
    # Handle help
    if [ "$action" = "help" ]; then
        print_banner
        show_usage
        exit 0
    fi
    
    print_banner
    check_docker
    check_compose

    # Check if agent exists for actions that require it
    if [ -n "$AGENT_NAME" ]; then
        local agent_dir=$(get_agent_dir)
        case "$action" in
            start|stop|clean|status|cli|tui|build)
                # These actions require the agent to already exist
                if [ ! -d "$agent_dir" ]; then
                    error "Agent '${AGENT_NAME}' does not exist"
                    echo "   Create it first with: ./docker-setup.sh ${AGENT_NAME}"
                    echo "   Or list existing agents: ./docker-setup.sh --list"
                    exit 1
                fi
                info "Using existing agent '${AGENT_NAME}' at ${agent_dir}"
                ;;
            "")
                # Full setup - create agent if it doesn't exist
                if [ ! -d "$agent_dir" ]; then
                    init_agent
                else
                    info "Using existing agent '${AGENT_NAME}' at ${agent_dir}"
                    # Regenerate compose file to apply any updates
                    generate_compose_file
                    # Ensure .env file is writable
                    chmod 666 "${agent_dir}/.env" 2>/dev/null || true
                fi
                ;;
        esac
    fi

    case "$action" in
        build)
            # Regenerate compose file to apply any updates
            if [ -n "$AGENT_NAME" ]; then
                generate_compose_file
                chmod 666 "$(get_agent_dir)/.env" 2>/dev/null || true
            fi
            build_images
            ;;
        start)
            setup_env
            start_databases
            start_gateway
            show_status
            ;;
        stop)
            stop_services
            ;;
        clean)
            clean_all
            ;;
        status)
            show_status
            ;;
        cli)
            if [ -n "$AGENT_NAME" ]; then
                compose run --rm ${AGENT_NAME}-cli "${cli_args[@]}"
            else
                $COMPOSE_CMD run --rm openagent-cli "${cli_args[@]}"
            fi
            ;;
        tui)
            step "Starting TUI chat interface (standalone - no gateway needed)..."
            if [ -n "$AGENT_NAME" ]; then
                compose run --rm --no-deps ${AGENT_NAME}-tui
            else
                $COMPOSE_CMD run --rm --no-deps openagent-tui
            fi
            ;;
        "")
            # Full setup
            setup_env
            build_images
            start_databases
            run_onboard
            echo ""
            step "Setup complete! 🎉"
            echo ""
            if [ -n "$AGENT_NAME" ]; then
                info "Agent '${AGENT_NAME}' is ready!"
                echo "   • Start gateway:      ./docker-setup.sh ${AGENT_NAME} --start"
                echo "   • Run TUI chat:       ./docker-setup.sh ${AGENT_NAME} --tui"
                echo "   • Run CLI chat:       ./docker-setup.sh ${AGENT_NAME} --cli chat"
                echo "   • Check status:       ./docker-setup.sh ${AGENT_NAME} --status"
                echo "   • View logs:          docker compose -f $(get_compose_file) logs -f"
                echo "   • List all agents:    ./docker-setup.sh --list"
            else
                info "Next steps:"
                echo "   • Start the gateway:  ./docker-setup.sh --start"
                echo "   • Run TUI chat:       ./docker-setup.sh --tui"
                echo "   • Run CLI chat:       ./docker-setup.sh --cli chat"
                echo "   • Check status:       ./docker-setup.sh --status"
                echo "   • View logs:          docker compose logs -f"
            fi
            echo ""
            ;;
        *)
            error "Unknown option"
            show_usage
            exit 1
            ;;
    esac
}

main "$@"
