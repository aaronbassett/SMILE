# Security Policy

## Reporting Security Vulnerabilities

If you discover a security vulnerability in SMILE Loop, please report it responsibly. **Do not open a public GitHub issue** for security vulnerabilities, as this could put users at risk.

### How to Report

1. **Email** the SMILE Loop security team at: **security@example.com**
   - Or open a private security advisory on GitHub: https://github.com/aaronbassett/SMILE/security/advisories

2. **Include in your report:**
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Any suggested fixes

3. **What happens next:**
   - We'll acknowledge receipt within 48 hours
   - We'll investigate and determine severity
   - We'll work on a fix and security patch
   - We'll notify you when a fix is released

### Responsible Disclosure Timeline

- **Day 0** - You report the vulnerability
- **Day 1** - We acknowledge and confirm receipt
- **Day 3-7** - We provide initial assessment
- **Day 7-30** - We develop and test a fix
- **Day 30+** - We release a security patch and public disclosure

We'll work with you to coordinate disclosure timing.

## Security Best Practices

### For Users

#### Environment Variables

SMILE Loop may be configured with sensitive credentials:

```bash
# Example: LLM API keys in Docker environment
docker run --env ANTHROPIC_API_KEY=sk-... smile-base:latest
```

**Best practices:**
- Use Docker secrets or environment variable files, not command-line flags
- Rotate API keys regularly
- Use minimal permission scopes for API credentials
- Don't commit credentials to version control

#### Docker Container Security

The `smile-base` Docker image contains LLM CLI tools that may process sensitive data:

```bash
# Use read-only filesystem when possible
docker run --read-only --tmpfs /tmp smile-base:latest

# Drop unnecessary capabilities
docker run --cap-drop=ALL smile-base:latest

# Run as non-root user
docker run --user 1000:1000 smile-base:latest
```

#### Network Security

SMILE Loop runs an HTTP server (default port 3000):

```bash
# The server binds to localhost by default:
# http://127.0.0.1:3000

# ⚠️ ALPHA LIMITATION: No authentication on WebSocket/HTTP endpoints
# Don't expose to untrusted networks
```

**Recommendations:**
- Only expose SMILE on trusted networks
- Use a firewall to restrict access
- Don't use in public CI/CD until authentication is added
- Run behind a proxy with authentication (nginx, haproxy)

### For Developers

#### Code Security

1. **Never commit secrets**
   - API keys, tokens, credentials
   - Private data or credentials
   - Use `.gitignore` for `.env` files

2. **Input validation**
   - Validate tutorial file content before processing
   - Validate configuration values
   - Sanitize LLM responses before using in commands

3. **Dependency scanning**
   - Keep dependencies up to date
   - Run `cargo audit` and `pip-audit` regularly
   - Review dependency changes in PRs

4. **Error handling**
   - Don't leak sensitive information in error messages
   - Log securely (redact API keys, tokens)
   - Don't expose internal paths in errors

#### Docker Image Security

The base Docker image must be:
- Built from trusted base images (official repositories)
- Scanned for vulnerabilities
- Kept up-to-date with security patches
- Minimal size (only necessary packages)

#### LLM CLI Integration

When invoking external LLM CLIs:

```python
# ✅ DO: Validate input before passing to LLM
validated_prompt = sanitize_tutorial_content(tutorial_markdown)
result = run_llm_cli(validated_prompt)

# ❌ DON'T: Pass raw user input directly
result = run_llm_cli(untrusted_tutorial)
```

## Known Security Limitations (Alpha)

SMILE Loop is an alpha release. Known security limitations:

1. **No Authentication**
   - HTTP API and WebSocket endpoints have no authentication
   - Anyone with network access can read/control the loop
   - Only use on trusted networks

2. **Container Isolation**
   - Agents run in Docker containers but don't have additional sandboxing
   - Compromised LLM could potentially escape container
   - Use additional isolation layers for untrusted tutorials

3. **Secrets Management**
   - API keys are passed via environment variables or config files
   - Logs may contain sensitive information
   - Implement secret redaction in production

4. **LLM CLI Execution**
   - LLM CLI tools are invoked with subprocess calls
   - Limited input validation before passing to LLM
   - Review LLM CLI documentation for security considerations

## Future Security Improvements

Planned improvements for future releases:

- [ ] Authentication and authorization for API endpoints
- [ ] Secrets management integration (HashiCorp Vault, AWS Secrets Manager)
- [ ] Enhanced input validation and sanitization
- [ ] Security audit of dependencies
- [ ] Vulnerability scanning in CI/CD
- [ ] Security documentation for deployment
- [ ] Rate limiting and DDoS protection
- [ ] Container security scanning
- [ ] Audit logging of sensitive operations

## Dependencies and Vulnerabilities

### Rust Dependencies

View the dependency tree:
```bash
cargo tree
```

Audit for known vulnerabilities:
```bash
cargo audit
```

### Python Dependencies

View installed packages:
```bash
cd python && pip list
```

Audit for known vulnerabilities:
```bash
pip-audit
```

## Compliance and Standards

SMILE Loop follows these security best practices:

- **OWASP Top 10** - Awareness of common vulnerabilities
- **CWE/SANS** - Common Weakness Enumeration
- **Secure Coding Guidelines** - Language-specific best practices
- **Supply Chain Security** - Dependency transparency and scanning

## Security Checklist

Before deploying SMILE Loop in production:

- [ ] Understand alpha security limitations (no auth, container isolation)
- [ ] Use only on trusted networks
- [ ] Protect LLM API credentials
- [ ] Run Docker with appropriate security constraints
- [ ] Implement network isolation/firewall rules
- [ ] Monitor logs for suspicious activity
- [ ] Keep all dependencies updated
- [ ] Review and customize configuration
- [ ] Test with non-sensitive tutorial data first
- [ ] Have an incident response plan

## Security Updates

We release security patches as needed:

- **Critical vulnerabilities** - Immediate patch release
- **High severity** - Patched in next release or as hotfix
- **Medium/Low** - Included in next regular release

Subscribe to security advisories:
- Watch this repository for releases
- GitHub Security Advisories: https://github.com/advisories
- Follow the project's security announcements

## Contact

- **Security Issues**: security@example.com (do not use for public issues) or private advisory
- **GitHub Issues**: https://github.com/aaronbassett/SMILE/issues (for non-security bugs)
- **GitHub Discussions**: https://github.com/aaronbassett/SMILE/discussions (for security questions)

---

Last Updated: February 3, 2026
Version: Alpha (0.1.0)
