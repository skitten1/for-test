import { Component, OnInit } from '@angular/core';
import { Router } from '@angular/router';
import { NbDialogService, NbToastrService } from '@nebular/theme';
import { LocalDataSource } from 'ng2-smart-table';
import { ClusterService, Cluster } from '../../../../@core/data/cluster.service';
import { ErrorHandler } from '../../../../@core/utils/error-handler';
import { ConfirmDialogService } from '../../../../@core/services/confirm-dialog.service';

@Component({
  selector: 'ngx-cluster-list',
  templateUrl: './cluster-list.component.html',
  styleUrls: ['./cluster-list.component.scss'],
})
export class ClusterListComponent implements OnInit {
  source: LocalDataSource = new LocalDataSource();
  loading = true;

  settings = {
    mode: 'external',
    hideSubHeader: false,  // Enable search
    noDataMessage: 'No cluster data. Click the button above to add a cluster.',
    actions: {
      columnTitle: 'Actions',
      add: false,
      edit: true,
      delete: true,
      position: 'right',
    },
    edit: {
      editButtonContent: '<i class="nb-edit"></i>',
    },
    delete: {
      deleteButtonContent: '<i class="nb-trash"></i>',
      confirmDelete: false,
    },
    pager: {
      display: true,
      perPage: 10,
    },
    columns: {
      id: {
        title: 'ID',
        type: 'number',
        width: '5%',
      },
      name: {
        title: 'Cluster Name',
        type: 'string',
      },
      fe_host: {
        title: 'FE Address',
        type: 'string',
      },
      fe_http_port: {
        title: 'HTTP Port',
        type: 'number',
        width: '10%',
      },
      fe_query_port: {
        title: 'Query Port',
        type: 'number',
        width: '10%',
      },
      username: {
        title: 'Username',
        type: 'string',
        width: '10%',
      },
      description: {
        title: 'Description',
        type: 'string',
      },
      created_at: {
        title: 'Created At',
        type: 'string',
        valuePrepareFunction: (date: string) => {
          return new Date(date).toLocaleString('zh-CN');
        },
      },
    },
  };

  constructor(
    private clusterService: ClusterService,
    private router: Router,
    private dialogService: NbDialogService,
    private toastrService: NbToastrService,
    private confirmDialogService: ConfirmDialogService,
  ) {}

  ngOnInit(): void {
    this.loadClusters();
  }

  loadClusters(): void {
    this.loading = true;
    this.clusterService.listClusters().subscribe({
      next: (clusters) => {
        this.source.load(clusters);
        this.loading = false;
      },
      error: (error) => {
        this.toastrService.danger(
          ErrorHandler.extractErrorMessage(error),
          'Error',
        );
        this.loading = false;
      },
    });
  }

  onCreate(): void {
    this.router.navigate(['/pages/starrocks/clusters/new']);
  }

  onEdit(event: any): void {
    this.router.navigate(['/pages/starrocks/clusters', event.data.id, 'edit']);
  }

  onDelete(event: any): void {
    const cluster = event.data as Cluster;

    this.confirmDialogService.confirmDelete(cluster.name)
      .subscribe(confirmed => {
        if (!confirmed) {
          return;
        }

        this.clusterService.deleteCluster(cluster.id).subscribe({
          next: () => {
            this.toastrService.success('Cluster deleted successfully.', 'Success');
            this.loadClusters();
          },
          error: (error) => {
            this.toastrService.danger(
              ErrorHandler.extractErrorMessage(error),
              'Error',
            );
          },
        });
      });
  }

  onRowSelect(event: any): void {
    this.router.navigate(['/pages/starrocks/clusters', event.data.id]);
  }

  testConnection(cluster: Cluster): void {
    this.clusterService.getHealth(cluster.id).subscribe({
      next: (health) => {
        if (health.status === 'healthy') {
          const details = health.checks.map(c => `${c.name}: ${c.message}`).join('\n');
          this.toastrService.success(details, 'Health check passed');
        } else if (health.status === 'warning') {
          const details = health.checks.map(c => `${c.name}: ${c.message}`).join('\n');
          this.toastrService.warning(details, 'Health check warning');
        } else {
          const details = health.checks.map(c => `${c.name}: ${c.message}`).join('\n');
          this.toastrService.danger(details, 'Health check failed');
        }
      },
      error: (error) => {
        this.toastrService.danger(
          ErrorHandler.extractErrorMessage(error),
          'Error',
        );
      },
    });
  }
}

