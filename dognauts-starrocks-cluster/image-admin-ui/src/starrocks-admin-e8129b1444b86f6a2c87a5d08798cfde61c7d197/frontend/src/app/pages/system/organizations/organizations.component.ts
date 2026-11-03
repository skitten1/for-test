import { Component, OnDestroy, OnInit } from '@angular/core';
import { NbDialogService, NbToastrService } from '@nebular/theme';
import { LocalDataSource } from 'ng2-smart-table';
import { Subject } from 'rxjs';
import { takeUntil } from 'rxjs/operators';

import {
  CreateOrganizationRequest,
  Organization,
  OrganizationService,
  UpdateOrganizationRequest,
} from '../../../@core/data/organization.service';
import { PermissionService } from '../../../@core/data/permission.service';
import { ErrorHandler } from '../../../@core/utils/error-handler';
import { ConfirmDialogService } from '../../../@core/services/confirm-dialog.service';
import { OrganizationsActionsCellComponent } from './table/actions-cell.component';
import {
  OrganizationFormDialogComponent,
  OrganizationFormDialogResult,
} from './organization-form/organization-form-dialog.component';
import { AuthService } from '../../../@core/data/auth.service';
import { UserService } from '../../../@core/data/user.service';

@Component({
  selector: 'ngx-organizations',
  templateUrl: './organizations.component.html',
  styleUrls: ['./organizations.component.scss'],
})
export class OrganizationsComponent implements OnInit, OnDestroy {
  source: LocalDataSource = new LocalDataSource();
  loading = false;
  private destroy$ = new Subject<void>();

  isSuperAdmin = false;
  hasListPermission = false;
  canCreateOrganization = false;
  canUpdateOrganization = false;
  canDeleteOrganization = false;

  settings = this.buildTableSettings();

  constructor(
    private organizationService: OrganizationService,
    private permissionService: PermissionService,
    private dialogService: NbDialogService,
    private confirmDialog: ConfirmDialogService,
    private toastrService: NbToastrService,
    private authService: AuthService,
    private userService: UserService,
  ) {}

  ngOnInit(): void {
    this.permissionService.permissions$
      .pipe(takeUntil(this.destroy$))
      .subscribe(() => this.applyPermissionState());

    this.applyPermissionState();
  }

  ngOnDestroy(): void {
    this.destroy$.next();
    this.destroy$.complete();
  }

  loadOrganizations(): void {
    if (!this.hasListPermission) {
      this.loading = false;
      this.source.load([]);
      return;
    }

    this.loading = true;
    this.organizationService.listOrganizations().subscribe({
      next: (organizations) => {
        this.source.load(organizations);
        this.loading = false;
      },
      error: (error) => {
        ErrorHandler.handleHttpError(error, this.toastrService);
        this.loading = false;
      },
    });
  }

  openCreateOrganization(): void {
    if (!this.canCreateOrganization) {
      return;
    }

    const dialogRef = this.dialogService.open(OrganizationFormDialogComponent, {
      context: {
        mode: 'create',
      },
      closeOnBackdropClick: false,
      autoFocus: false,
    });

    dialogRef.onClose.subscribe((result?: OrganizationFormDialogResult) => {
      if (!result) {
        return;
      }
      this.createOrganization(result);
    });
  }

  openEditOrganization(organization: Organization): void {
    if (!this.canUpdateOrganization || organization.is_system) {
      return;
    }

    // First, get the full organization details (including admin_user_id)
    this.organizationService.getOrganization(organization.id).subscribe({
      next: (fullOrganization) => {
        // Then load users for this organization
        this.userService.listUsers().subscribe({
          next: (allUsers) => {
            // Filter users belonging to this organization
            const organizationUsers = allUsers.filter(u => u.organization_id === fullOrganization.id);
            
            const dialogRef = this.dialogService.open(OrganizationFormDialogComponent, {
              context: {
                mode: 'edit',
                organization: fullOrganization,
                availableUsers: organizationUsers,
              },
              closeOnBackdropClick: false,
              autoFocus: false,
            });

            dialogRef.onClose.subscribe((result?: OrganizationFormDialogResult) => {
              if (!result) {
                return;
              }
              this.updateOrganization(fullOrganization.id, result);
            });
          },
          error: (error) => {
            ErrorHandler.handleHttpError(error, this.toastrService);
          }
        });
      },
      error: (error) => {
        ErrorHandler.handleHttpError(error, this.toastrService);
      }
    });
  }

  deleteOrganization(organization: Organization): void {
    if (!this.canDeleteOrganization || organization.is_system) {
      return;
    }

    this.confirmDialog
      .confirmDelete(organization.name, 'This action cannot be undone. All data belonging to this organization will be deleted.')
      .subscribe((confirmed) => {
        if (confirmed) {
          this.performDelete(organization.id);
        }
      });
  }

  private createOrganization(result: OrganizationFormDialogResult): void {
    const payload: CreateOrganizationRequest = {
      code: result.code,
      name: result.name,
      description: result.description,
    };

    this.loading = true;
    this.organizationService.createOrganization(payload).subscribe({
      next: () => {
        this.toastrService.success('Organization created successfully.', 'Success');
        this.loadOrganizations();
      },
      error: (error) => {
        ErrorHandler.handleHttpError(error, this.toastrService);
        this.loading = false;
      },
    });
  }

  private updateOrganization(id: number, result: OrganizationFormDialogResult): void {
    const payload: UpdateOrganizationRequest = {
      name: result.name,
      description: result.description,
      admin_user_id: result.admin_user_id,
    };

    this.loading = true;
    this.organizationService.updateOrganization(id, payload).subscribe({
      next: () => {
        this.toastrService.success('Organization updated successfully.', 'Success');
        this.loadOrganizations();
      },
      error: (error) => {
        ErrorHandler.handleHttpError(error, this.toastrService);
        this.loading = false;
      },
    });
  }

  private performDelete(id: number): void {
    this.loading = true;
    this.organizationService.deleteOrganization(id).subscribe({
      next: () => {
        this.toastrService.success('Organization deleted.', 'Success');
        this.loadOrganizations();
      },
      error: (error) => {
        ErrorHandler.handleHttpError(error, this.toastrService);
        this.loading = false;
      },
    });
  }

  private applyPermissionState(): void {
    this.isSuperAdmin = this.authService.isSuperAdmin();
    this.hasListPermission =
      this.permissionService.hasPermission('api:organizations:list') || this.isSuperAdmin;
    this.canCreateOrganization =
      this.permissionService.hasPermission('api:organizations:create') || this.isSuperAdmin;
    this.canUpdateOrganization =
      this.permissionService.hasPermission('api:organizations:update') || this.isSuperAdmin;
    this.canDeleteOrganization =
      this.permissionService.hasPermission('api:organizations:delete') || this.isSuperAdmin;

    // Refresh table settings so action buttons pick up latest permissions
    this.settings = this.buildTableSettings();

    if (this.hasListPermission) {
      this.loadOrganizations();
    }
  }

  private buildTableSettings(): any {
    return {
      actions: {
        add: false,
        edit: false,
        delete: false,
        position: 'right',
      },
      columns: {
        code: {
          title: 'Organization Code',
          type: 'string',
          width: '15%',
        },
        name: {
          title: 'Organization Name',
          type: 'string',
          width: '20%',
        },
        description: {
          title: 'Description',
          type: 'string',
          width: '30%',
        },
        is_system: {
          title: 'System Organization',
          type: 'html',
          width: '10%',
          valuePrepareFunction: (cell: boolean) => {
            return cell
              ? '<span class="badge badge-danger">Yes</span>'
              : '<span class="badge badge-basic">No</span>';
          },
        },
        created_at: {
          title: 'Created At',
          type: 'string',
          width: '15%',
          valuePrepareFunction: (cell: string) => {
            return cell ? new Date(cell).toLocaleString('zh-CN') : '';
          },
        },
        actions: {
          title: 'Actions',
          type: 'custom',
          width: '10%',
          filter: false,
          sort: false,
          renderComponent: OrganizationsActionsCellComponent,
          onComponentInitFunction: (instance: OrganizationsActionsCellComponent) => {
            instance.edit.subscribe((organization) => {
              this.openEditOrganization(organization);
            });
            instance.delete.subscribe((organization) => {
              this.deleteOrganization(organization);
            });
          },
        },
      },
    };
  }
}

