import { Component, OnInit, OnDestroy } from '@angular/core';
import { interval, Subject } from 'rxjs';
import { takeUntil, switchMap } from 'rxjs/operators';
import { NbToastrService } from '@nebular/theme';
import { LocalDataSource } from 'ng2-smart-table';
import { NodeService, Backend } from '../../../@core/data/node.service';
import { ClusterService, Cluster } from '../../../@core/data/cluster.service';
import { ClusterContextService } from '../../../@core/data/cluster-context.service';
import { ErrorHandler } from '../../../@core/utils/error-handler';
import { ConfirmDialogService } from '../../../@core/services/confirm-dialog.service';
import { MetricThresholds, renderMetricBadge } from '../../../@core/utils/metric-badge';

@Component({
  selector: 'ngx-backends',
  templateUrl: './backends.component.html',
  styleUrls: ['./backends.component.scss'],
})
export class BackendsComponent implements OnInit, OnDestroy {
  source: LocalDataSource = new LocalDataSource();
  clusterId: number;
  activeCluster: Cluster | null = null;
  clusterName: string = '';
  deploymentMode: string = '';
  pageTitle: string = 'Backend Nodes';
  deploymentModeText: string = '';
  deploymentModeBadgeClass: string = '';
  loading = true;
  private destroy$ = new Subject<void>();
  private readonly diskThresholds: MetricThresholds = { warn: 70, danger: 85 };
  private readonly cpuThresholds: MetricThresholds = { warn: 60, danger: 85 };
  private readonly memoryThresholds: MetricThresholds = { warn: 65, danger: 85 };

  settings = {
    mode: 'external',
    hideSubHeader: false, // Enable search
    noDataMessage: 'No compute-node data available.',
    actions: {
      columnTitle: 'Actions',
      add: false,
      edit: false,
      delete: true,
      position: 'right',
    },
    delete: {
      deleteButtonContent: '<i class="nb-trash"></i>',
      confirmDelete: true,  // Enable custom confirmation via deleteConfirm event
    },
    pager: {
      display: true,
      perPage: 15,
    },
    columns: {
      BackendId: {
        title: 'Node ID',
        type: 'string',
        width: '8%',
      },
      IP: {
        title: 'Host',
        type: 'string',
      },
      HeartbeatPort: {
        title: 'Heartbeat Port',
        type: 'string',
        width: '10%',
      },
      BePort: {
        title: 'Service Port',
        type: 'string',
        width: '10%',
      },
      Alive: {
        title: 'Status',
        type: 'html',
        width: '8%',
        valuePrepareFunction: (value: string) => {
          const status = value === 'true' ? 'success' : 'danger';
          const text = value === 'true' ? 'Online' : 'Offline';
          return `<span class="badge badge-${status}">${text}</span>`;
        },
      },
      TabletNum: {
        title: 'Tablet Count',
        type: 'string',
        width: '10%',
      },
      DataUsedCapacity: {
        title: 'Used Storage',
        type: 'string',
      },
      TotalCapacity: {
        title: 'Total Storage',
        type: 'string',
      },
      UsedPct: {
        title: 'Disk Usage',
        type: 'html',
        width: '10%',
        valuePrepareFunction: (value: string) => renderMetricBadge(value, this.diskThresholds),
      },
      CpuUsedPct: {
        title: 'CPU Usage',
        type: 'html',
        width: '12%',
        valuePrepareFunction: (value: string) => renderMetricBadge(value, this.cpuThresholds),
      },
      MemUsedPct: {
        title: 'Memory Usage',
        type: 'html',
        width: '10%',
        valuePrepareFunction: (value: string) => renderMetricBadge(value, this.memoryThresholds),
      },
      NumRunningQueries: {
        title: 'Running Queries',
        type: 'string',
        width: '10%',
      },
    },
  };

  onDeleteConfirm(event: any): void {
    const backend = event.data;
    const itemName = `${backend.IP}:${backend.HeartbeatPort}`;
    const nodeType = this.deploymentMode === 'shared_data' ? 'CN (Compute Node)' : 'BE (Backend)';
    const additionalWarning = `⚠️ Warning: Removing a ${nodeType} node is a dangerous operation. Make sure that:\n1. The node data has been migrated.\n2. The cluster has enough replicas.\n3. The node has been stopped.`;
    
    this.confirmDialogService.confirmDelete(itemName, additionalWarning)
      .subscribe(confirmed => {
        if (!confirmed) {
          event.confirm.reject();
          return;
        }

        this.nodeService.deleteBackend(backend.IP, backend.HeartbeatPort)
          .subscribe({
            next: () => {
              this.toastrService.success(
                `${nodeType} node ${itemName} was deleted.`,
                'Success'
              );
              event.confirm.resolve();
              this.loadBackends();
            },
            error: (error) => {
              this.toastrService.danger(
                ErrorHandler.extractErrorMessage(error),
                'Deletion failed.',
              );
              event.confirm.reject();
            },
          });
      });
  }

  constructor(
    private nodeService: NodeService,
    private clusterService: ClusterService,
    private clusterContext: ClusterContextService,
    private toastrService: NbToastrService,
    private confirmDialogService: ConfirmDialogService,
  ) {
    // Get clusterId from ClusterContextService
    this.clusterId = this.clusterContext.getActiveClusterId() || 0;
  }

  ngOnInit(): void {
    // Subscribe to active cluster changes
    // activeCluster$ is a BehaviorSubject, so it emits immediately on subscribe
    this.clusterContext.activeCluster$
      .pipe(takeUntil(this.destroy$))
      .subscribe(cluster => {
        this.activeCluster = cluster;
        if (cluster) {
          // Always use the active cluster (override route parameter)
          const newClusterId = cluster.id;
          if (this.clusterId !== newClusterId) {
            this.clusterId = newClusterId;
            this.loadClusterInfo();
            this.loadBackends();
          }
        }
        // Backend will handle "no active cluster" case
        
      });

    // Load data - backend will get active cluster automatically
    // This ensures data loads even if activeCluster$ hasn't emitted yet
    this.loadClusterInfo();
    this.loadBackends();
  }

  ngOnDestroy(): void {
    this.destroy$.next();
    this.destroy$.complete();
  }

  loadClusterInfo(): void {
    this.clusterService.getCluster(this.clusterId)
      .pipe(takeUntil(this.destroy$))
      .subscribe({
        next: (cluster) => {
          this.clusterName = cluster.name;
          this.deploymentMode = cluster.deployment_mode || 'shared_nothing';
          
          // Update page title and badge based on deployment mode
          if (this.deploymentMode === 'shared_data') {
            this.pageTitle = 'Compute Nodes (CN)';
            this.deploymentModeText = 'Shared-Data';
            this.deploymentModeBadgeClass = 'badge-info';
          } else {
            this.pageTitle = 'Backend Nodes (BE)';
            this.deploymentModeText = 'Shared-Nothing';
            this.deploymentModeBadgeClass = 'badge-success';
          }
        },
      });
  }

  loadBackends(): void {
    this.loading = true;
    
    this.nodeService.listBackends()
      .pipe(takeUntil(this.destroy$))
      .subscribe({
        next: (backends) => {
          this.source.load(backends);
          this.loading = false;
        },
        error: (error) => {
          this.toastrService.danger(
            ErrorHandler.handleClusterError(error),
            'Error',
          );
          this.source.load([]);
          this.loading = false;
        },
      });
  }
}