#!/usr/bin/env node
// Apply the Dognauts platform-mode frontend changes to the pinned upstream tree.
const fs = require('fs');
const path = require('path');
const root = path.resolve(process.argv[2] || '.');

function replaceOnce(relativePath, oldText, newText) {
  const file = path.join(root, relativePath);
  let source = fs.readFileSync(file, 'utf8');
  const first = source.indexOf(oldText);
  if (first < 0 || source.indexOf(oldText, first + oldText.length) >= 0) {
    throw new Error(`${relativePath}: expected exactly one source anchor; refusing partial patch`);
  }
  source = source.slice(0, first) + newText + source.slice(first + oldText.length);
  fs.writeFileSync(file, source, 'utf8');
}

replaceOnce(
  'frontend/src/app/@core/data/auth.service.ts',
  `  public isSuperAdmin(): boolean {
    const user = this.currentUserSubject.value;`,
  `  public isPlatformMode(): boolean {
    const path = window.location?.pathname || '';
    const manualFallback = new URLSearchParams(window.location?.search || '').get('local') === '1';
    return path.includes('/starrocks/admin-ui') && !manualFallback && !this.token;
  }

  public isSuperAdmin(): boolean {
    if (this.isPlatformMode()) return true;
    const user = this.currentUserSubject.value;`,
);

replaceOnce(
  'frontend/src/app/@core/data/auth.service.ts',
  `    const storedUser = localStorage.getItem('current_user');
    this.currentUserSubject = new BehaviorSubject<User | null>(
      storedUser ? JSON.parse(storedUser) : null,
    );`,
  `    const storedUser = localStorage.getItem('current_user');
    const platformUser: User = {
      id: 1,
      username: 'admin',
      email: 'admin@example.com',
      created_at: new Date().toISOString(),
      organization_id: 1,
      is_super_admin: true,
      is_org_admin: false,
      first_log: false,
    };
    this.currentUserSubject = new BehaviorSubject<User | null>(
      this.isPlatformMode() ? platformUser : (storedUser ? JSON.parse(storedUser) : null),
    );`,
);

replaceOnce(
  'frontend/src/app/@core/data/auth.service.ts',
  `  isAuthenticated(): boolean {
    return !!this.token;
  }

  getMe(): Observable<User> {
    return this.api.get<User>('/auth/me');
  }`,
  `  isAuthenticated(): boolean {
    return this.isPlatformMode() || !!this.token;
  }

  getMe(): Observable<User> {
    return this.isPlatformMode()
      ? of(this.currentUserSubject.value as User)
      : this.api.get<User>('/auth/me');
  }`,
);

replaceOnce(
  'frontend/src/app/@core/data/permission.service.ts',
  `  /**
   * Initialize permissions from API
   * Should be called after user login
   */
  initPermissions(): Observable<Permission[]> {
    if (this.initialized) {`,
  `  private isPlatformMode(): boolean {
    const path = window.location?.pathname || '';
    const manualFallback = new URLSearchParams(window.location?.search || '').get('local') === '1';
    return path.includes('/starrocks/admin-ui') && !manualFallback && !localStorage.getItem('jwt_token');
  }

  /**
   * Initialize permissions from API
   * Should be called after user login
   */
  initPermissions(): Observable<Permission[]> {
    if (this.isPlatformMode()) {
      this.initialized = true;
      this.permissionsSubject.next([]);
      return of([]);
    }
    if (this.initialized) {`,
);

replaceOnce(
  'frontend/src/app/@core/data/permission.service.ts',
  `  hasPermission(code: string, action?: string): boolean {
    const permissions = this.permissionsSubject.value;`,
  `  hasPermission(code: string, action?: string): boolean {
    if (this.isPlatformMode()) return true;
    const permissions = this.permissionsSubject.value;`,
);

replaceOnce(
  'frontend/src/app/@core/interceptors/jwt.interceptor.ts',
  `        setHeaders: {
          Authorization: \`Bearer \${token}\`,
        },`,
  `        setHeaders: {
          'X-SR-Admin-Token': \`Bearer \${token}\`,
        },`,
);

replaceOnce(
  'frontend/src/app/@core/data/api.service.ts',
  `  private detectBasePath(): string {
    const path = window.location.pathname || '';
    const markers = ['/pages/', '/auth/', '/login', '/register', '/reset'];`,
  `  private detectBasePath(): string {
    const path = window.location.pathname || '';
    const adminUiMarker = '/starrocks/admin-ui';
    const adminUiIndex = path.indexOf(adminUiMarker);
    if (adminUiIndex >= 0) {
      return path.substring(0, adminUiIndex + adminUiMarker.length).replace(/\\/$/, '');
    }
    const markers = ['/pages/', '/auth/', '/login', '/register', '/reset'];`,
);

process.stdout.write(`Applied frontend platform-mode changes under ${root}\n`);
