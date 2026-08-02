## Features

- **TypeScript** - For type safety and improved developer experience
- **TanStack Router** - File-based routing with full type safety
- **React Native** - Build mobile apps using React
- **Expo** - Tools for React Native development
- **TailwindCSS** - Utility-first CSS for rapid UI development
- **shadcn/ui** - Reusable UI components
- **Elysia** - Type-safe, high-performance framework
- **oRPC** - End-to-end type-safe APIs with OpenAPI integration
- **Bun** - Runtime environment
- **Drizzle** - TypeScript-first ORM
- **PostgreSQL** - Database engine
- **Authentication** - Better-Auth
- **Husky** - Git hooks for code quality
- **Oxlint** - Oxlint + Oxfmt (linting & formatting)
- **Changesets** - Versioning and publishing
- **Turborepo** - Optimized monorepo build system.
- **Docker Sandboxes** - Run OpenCode in an isolated microVM with clone-mode Git workflow

## Getting Started

First, copy the example environment variables file:

```bash
cp .env.example .env
```

Then, install the dependencies:

```bash
bun install
```

and

```bash
bun run prepare
```

Then, run the following command to start the database

```bash
bun run docker:dev
```

Then, run the following command to start the development server, build the web application and run the migrations:

```bash
bun run dev
```

Then, run the following command to seed the database:

```bash
bun run db-seed
```

Dev users: `admin@lindaflor.com`, `org-alpha-owner@lindaflor.com`, `org-alpha-admin@lindaflor.com`, etc. Same password for all (default: `password`; override with `SEED_DEV_PASSWORD`). Requires `NODE_ENV=development`.

Open [http://localhost:9001](http://localhost:9001) in your browser to see the web application.
The API is running at [http://localhost:9000](http://localhost:9000).

## Project Structure

```
lindaflor/
├── docs/            # Documentation
├── apps/
│   ├── web/         # Frontend application (React + TanStack Router)
│   ├── native/      # Mobile application (React Native, Expo)
│   └── server/      # Backend API (Elysia, ORPC)
├── packages/
│   ├── api/         # API layer / business logic
│   ├── auth/        # Authentication configuration & logic
│   ├── config/      # TypeScript configuration files
│   ├── db/          # Database schema & queries
│   ├── env/         # Environment variable
│   ├── mail/        # Mail configuration & logic
│   └── valkey/      # Valkey (Redis-compatible) in-memory data store
```

## Available Scripts

- `bun run dev`: Start all applications in development mode
- `bun run build`: Build all applications
- `bun run check`: Run Oxlint and Oxfmt
- `bun run check-types`: Check TypeScript types across all apps
- `bun run clean`: Clean the projects artifacts
- `bun run clean:root`: Clean the root projects artifacts
- `bun run test`: Run the tests
- `bun run version`: Create a new version
- `bun run dev:web`: Start only the web application
- `bun run dev:server`: Start only the server
- `bun run db:push`: Push schema changes to database
- `bun run db:studio`: Open database studio UI
- `bun run db:migrate`: Run the migrations
- `bun run db:generate`: Generate a new migration
- `bun run db-seed`: Seed the database with sample data (development only)
- `bun run docker:dev`: Start the database and run the migrations and seed the database
- `bun run docker:build`: Build the Docker images
- `bun run docker:down`: Stop and remove the Docker containers
- `bun run prebuild`: Run Expo prebuild for native apps (iOS/Android)
- `bun run dev-android`: Start the native Android app in development mode
- `bun run docker:clean`: Stop and remove the Docker containers and clean up the Docker system
