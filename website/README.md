# ICN Website

The official website for the InterCooperative Network (ICN) - a federated compute, resource, and governance protocol designed for cooperatives, communities, and federations.

## 🌟 Features

- **Modern Design**: Clean, responsive design with dark theme and vibrant accents
- **Comprehensive Documentation**: Integrated documentation with clear navigation and search
- **Performance Optimized**: Built with Astro for fast loading and excellent SEO
- **Accessibility**: WCAG compliant with proper semantic markup and keyboard navigation
- **Mobile First**: Responsive design that works beautifully on all devices

## 🚀 Quick Start

### Prerequisites

- Node.js 18+ 
- npm or yarn

### Installation

```bash
# Clone the repository
git clone https://github.com/InterCooperative-Network/icn-website.git
cd icn-website

# Install dependencies
npm install

# Start development server
npm run dev
```

The website will be available at `http://localhost:4321`

## 📁 Project Structure

```
icn-website/
├── public/
│   ├── images/           # Static images and SVG assets
│   └── favicon.svg       # Site favicon
├── src/
│   ├── components/       # Reusable Astro components
│   ├── layouts/          # Page layouts
│   ├── pages/            # Site pages and routes
│   │   ├── docs/         # Documentation pages
│   │   └── blog/         # Blog posts
│   └── styles/           # Global CSS and design system
├── astro.config.mjs      # Astro configuration
├── tailwind.config.mjs   # Tailwind CSS configuration
└── package.json          # Dependencies and scripts
```

## 🎨 Design System

The website uses a modern design system with:

- **Typography**: Inter for body text, Lexend for headings
- **Colors**: Dark theme with teal/blue accent palette
- **Components**: Reusable button, card, and layout components
- **Animations**: Subtle scroll animations and hover effects
- **Responsive**: Mobile-first design with Tailwind CSS

### Color Palette

- **Primary**: `#00D4AA` (Teal)
- **Secondary**: `#3B82F6` (Blue) 
- **Accent**: `#32FFD2` (Bright Teal)
- **Purple**: `#8B5CF6` (Purple)
- **Background**: `#0A0E1A` (Dark Navy)

## 📚 Documentation Integration

The website includes comprehensive documentation with:

- **Getting Started**: Quick setup guides and tutorials
- **Core Features**: Deep dives into ICN's capabilities
- **API Reference**: Complete API documentation
- **RFCs**: Technical specifications and proposals
- **Developer Tools**: CLI guides and development resources

## 🛠 Development

### Available Scripts

```bash
npm run dev        # Start development server
npm run build      # Build for production
npm run preview    # Preview production build
npm run lint       # Run Astro checks
npm run format     # Format code with Prettier
npm run deploy     # Deploy to GitHub Pages
```

### Adding Content

#### New Pages
Create `.astro` files in `src/pages/` directory. The file structure maps to URL routes.

#### Blog Posts
Add new blog posts in `src/pages/blog/` directory with frontmatter for metadata.

#### Documentation
Add documentation pages in `src/pages/docs/` directory with proper navigation.

### Styling Guidelines

- Use Tailwind CSS utility classes for styling
- Follow the established design system colors and typography
- Ensure responsive design with mobile-first approach
- Add hover states and transitions for interactive elements

## 🚀 Deployment

The website is automatically deployed to GitHub Pages when changes are pushed to the main branch.

For manual deployment:

```bash
npm run deploy
```

## 🤝 Contributing

We welcome contributions to improve the website! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test thoroughly
5. Submit a pull request

### Content Guidelines

- Keep content clear and accessible
- Use proper semantic HTML
- Ensure all images have alt text
- Test on multiple devices and browsers
- Follow the established tone and style

## 📄 License

This project is licensed under the same terms as the ICN project. See the main repository for details.

## 🔗 Links

- **Main Repository**: [icn](https://github.com/InterCooperative-Network/icn)
- **Documentation**: [ICN Docs](https://github.com/InterCooperative-Network/icn/tree/main/docs)
- **Community**: [Discussions](https://github.com/InterCooperative-Network/icn/discussions)
- **Issues**: [Bug Reports](https://github.com/InterCooperative-Network/icn-website/issues)

---

Built with ❤️ by the ICN community
