# Registry API - Deployment Checklist

## Pre-Deployment Verification

### Code Quality

- [ ] All tests passing: `npm test`
- [ ] Linter clean: `npm run lint`
- [ ] Build succeeds: `npm run build`
- [ ] No console errors in dev
- [ ] No security warnings: `npm audit`
- [ ] TypeScript strict mode enabled
- [ ] All TODOs reviewed

### Configuration

- [ ] `.env.example` updated with all required variables
- [ ] `.env` file created and configured
- [ ] JWT_SECRET is random, 32+ characters
- [ ] MONGODB_URI points to production DB
- [ ] NODE_ENV=production
- [ ] CORS_ORIGIN set to production domain
- [ ] All endpoints tested with production config

### Security Audit

- [ ] Passwords hashed with bcrypt
- [ ] JWT tokens expire after 7 days
- [ ] HTTPS/TLS configured
- [ ] CORS restricted to specific origins
- [ ] Rate limiting in place
- [ ] Input validation on all endpoints
- [ ] No secrets in code or git
- [ ] API keys rotated
- [ ] Security headers enabled (Helmet)

### Database

- [ ] MongoDB instance running
- [ ] Database connection tested
- [ ] Backup strategy defined
- [ ] Replication configured (if needed)
- [ ] User authentication enabled
- [ ] Database size monitored
- [ ] Index creation optimized

### Testing

- [ ] Unit tests: 100% passing
- [ ] Integration tests: All workflows tested
- [ ] Manual testing: All features verified
- [ ] Edge cases tested
- [ ] Error scenarios tested
- [ ] Load testing: 100+ concurrent users
- [ ] Stress testing: Capacity verified

### Documentation

- [ ] README.md complete
- [ ] QUICK_START.md available
- [ ] API documentation up to date
- [ ] Deployment guide written
- [ ] Troubleshooting guide included
- [ ] Environment variables documented
- [ ] API endpoints documented

### Performance

- [ ] Search responds < 500ms
- [ ] Download completes < 30s
- [ ] Web UI loads < 2s
- [ ] Supports 100+ concurrent users
- [ ] Memory usage monitored
- [ ] CPU usage reasonable
- [ ] Database queries optimized

## Deployment Steps

### Docker Deployment

#### 1. Build Docker Image

```bash norun
docker build -t starforge-registry:latest .
docker tag starforge-registry:latest starforge-registry:$(date +%Y%m%d-%H%M%S)
docker push your-registry/starforge-registry:latest
```

- [ ] Build succeeds
- [ ] Image size reasonable (< 500MB)
- [ ] No build warnings

#### 2. Configure Deployment Environment

```bash norun
# Set environment variables on server
export NODE_ENV=production
export JWT_SECRET=$(head -c 32 /dev/urandom | base64)
export MONGODB_URI=<production-db-connection>
export PORT=3000
```

- [ ] All env vars set
- [ ] Secrets properly secured
- [ ] No hardcoded credentials

#### 3. Deploy Container

```bash norun
docker run -d \
  -p 3000:3000 \
  --name starforge-registry \
  -e NODE_ENV=production \
  -e JWT_SECRET=$JWT_SECRET \
  -e MONGODB_URI=$MONGODB_URI \
  --restart unless-stopped \
  starforge-registry:latest
```

- [ ] Container starts successfully
- [ ] Health check passes: `curl http://localhost:3000/health`
- [ ] Logs show no errors

### Traditional Node.js Deployment

#### 1. Install Production Dependencies

```bash norun
npm install --production
npm run build
```

- [ ] Dependencies installed
- [ ] Build completes

#### 2. Start with Process Manager (PM2)

```bash norun
npm install -g pm2
pm2 start npm --name "registry-api" -- start
pm2 save
pm2 startup
```

- [ ] Process starts
- [ ] Auto-restarts on crash
- [ ] Loads on server reboot

### Cloud Platform Deployment

#### Heroku

```bash norun
git push heroku main
heroku config:set JWT_SECRET=<secret>
heroku config:set MONGODB_URI=<url>
heroku logs --tail
```

- [ ] Deployment succeeds
- [ ] Logs clean
- [ ] Health check passes

#### AWS Lambda (Serverless)

- [ ] Serverless framework installed
- [ ] API Gateway configured
- [ ] RDS/DocumentDB for MongoDB
- [ ] Environment variables set
- [ ] Test deployment
- [ ] CloudWatch monitoring

#### DigitalOcean/Linode

- [ ] SSH access configured
- [ ] Node.js installed
- [ ] PM2 or systemd configured
- [ ] SSL certificate installed
- [ ] Firewall rules set
- [ ] Monitoring enabled

### HTTPS/TLS Setup

#### 1. Obtain Certificate

```bash norun
# Let's Encrypt with Certbot
sudo certbot certonly --standalone -d registry.starforge.dev

# Copy to accessible location
sudo cp /etc/letsencrypt/live/registry.starforge.dev/*.pem ./certs/
```

- [ ] Certificate obtained
- [ ] Files accessible to application
- [ ] Renewal automated

#### 2. Configure Node App

- [ ] HTTPS options configured
- [ ] Certificate paths in code
- [ ] HTTP redirects to HTTPS
- [ ] HSTS headers enabled

#### 3. Verify HTTPS

```bash norun
curl https://registry.starforge.dev/health
```

- [ ] HTTPS works
- [ ] No certificate warnings
- [ ] Redirect from HTTP works

### DNS Configuration

```bash norun
# Point domain to server
registry.starforge.dev A 12.34.56.78

# DNS propagation test
nslookup registry.starforge.dev
```

- [ ] A record created
- [ ] DNS resolves
- [ ] TTL reasonable (300-3600)

### Monitoring & Logging

#### 1. Application Logging

```bash norun
# Configure log file
mkdir -p /var/log/starforge-registry
touch /var/log/starforge-registry/app.log
chmod 666 /var/log/starforge-registry/app.log
```

- [ ] Log file created
- [ ] Rotation configured
- [ ] Permissions correct

#### 2. Health Monitoring

```bash norun
# Monitor endpoint
curl -s http://localhost:3000/health | jq .

# Set up periodic health check
* * * * * curl -s http://localhost:3000/health || alert
```

- [ ] Health endpoint working
- [ ] Monitoring configured
- [ ] Alerts set up

#### 3. Performance Monitoring

- [ ] New Relic OR
- [ ] DataDog OR
- [ ] CloudWatch
      Configured with:
- [ ] Application performance metrics
- [ ] Database query times
- [ ] Error rates
- [ ] Uptime monitoring

### Backup & Recovery

#### 1. Database Backups

```bash norun
# MongoDB backup
mongodump --uri="mongodb://..." --out=/backups/mongodb/$(date +%Y%m%d)

# Schedule daily backup
0 2 * * * mongodump --uri="..." --out=/backups/mongodb/$(date +\%Y\%m\%d)
```

- [ ] Backup script created
- [ ] Scheduled daily
- [ ] Tested restore process

#### 2. File Backups

```bash norun
# Backup storage directory
tar -czf /backups/templates-$(date +%Y%m%d).tar.gz /storage/templates/
```

- [ ] Storage backed up
- [ ] Restore tested
- [ ] Retention policy set

### Rate Limiting

#### 1. Install Middleware

```bash norun
npm install express-rate-limit
```

#### 2. Configure Limits

```javascript
const rateLimit = require("express-rate-limit");
const limiter = rateLimit({
  windowMs: 15 * 60 * 1000,
  max: 100,
});
app.use("/api/", limiter);
```

- [ ] Rate limiter installed
- [ ] Configured on API routes
- [ ] Tested with load

## Post-Deployment Verification

### Functionality Tests

```bash norun
# Test signup
curl -X POST https://registry.starforge.dev/api/auth/signup \
  -d '{"email":"test@example.com","username":"test","password":"password123"}'

# Test search
curl -X POST https://registry.starforge.dev/api/templates/search \
  -d '{"query":""}'

# Test web UI
curl https://registry.starforge.dev/
```

- [ ] Signup works
- [ ] Search works
- [ ] Web UI loads
- [ ] API responds with correct data

### Performance Tests

```bash norun
# Load test
ab -n 1000 -c 100 https://registry.starforge.dev/health

# Measure response time
curl -w "Time: %{time_total}s\n" https://registry.starforge.dev/health
```

- [ ] Handles concurrent requests
- [ ] Response time acceptable
- [ ] No timeouts

### Security Tests

```bash norun
# Test HTTPS
curl -I https://registry.starforge.dev/

# Check certificate
echo | openssl s_client -servername registry.starforge.dev \
  -connect registry.starforge.dev:443

# Test CORS headers
curl -H "Origin: https://example.com" \
  -H "Access-Control-Request-Method: GET" \
  https://registry.starforge.dev/
```

- [ ] HTTPS enforced
- [ ] Certificate valid
- [ ] CORS configured correctly
- [ ] Security headers present

### Monitoring Tests

- [ ] Health endpoint responds
- [ ] Logging working
- [ ] Alerts configured
- [ ] Dashboards populated
- [ ] Error tracking working

## Rollback Plan

### If Deployment Fails

1. [ ] Stop current deployment
2. [ ] Restore previous version from git
3. [ ] Rollback database if needed
4. [ ] Verify health checks
5. [ ] Notify stakeholders

### Quick Rollback Commands

```bash norun
# Docker rollback
docker stop starforge-registry
docker run -d \
  --name starforge-registry \
  starforge-registry:previous-tag

# Git rollback
git checkout previous-tag
npm run build
npm start

# PM2 rollback
pm2 restart registry-api
```

## Maintenance Schedule

### Daily

- [ ] Check health endpoint
- [ ] Review error logs
- [ ] Monitor performance metrics

### Weekly

- [ ] Review analytics
- [ ] Update dependencies
- [ ] Run backup verification

### Monthly

- [ ] Security audit
- [ ] Performance review
- [ ] Database optimization
- [ ] Capacity planning

### Quarterly

- [ ] Major dependency updates
- [ ] Security assessment
- [ ] Disaster recovery drill
- [ ] Cost analysis

## Sign-Off

- [ ] Tech Lead approval
- [ ] Security review passed
- [ ] Performance acceptable
- [ ] Documentation complete
- [ ] Team trained
- [ ] Monitoring active
- [ ] Rollback plan ready

**Deployed by:** ******\_****** **Date:** ******\_******

**Status:** ☐ Ready for Production ☐ Needs Fixes ☐ On Hold

**Notes:**
