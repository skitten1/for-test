import { Component, EventEmitter, Input, Output } from '@angular/core';

interface RoleRow {
  id: number;
  name: string;
  is_system: boolean;
}

@Component({
  selector: 'ngx-roles-configure-cell',
  template: `
    <button
      nbButton
      size="tiny"
      status="info"
      [disabled]="rowData?.is_system"
      (click)="configure.emit(rowData)"
      nbTooltip="Configure Permissions"
    >
      <nb-icon icon="settings-2-outline"></nb-icon>
      Configure Permissions
    </button>
  `,
})
export class RolesConfigureCellComponent {
  @Input() value: any;
  @Input() rowData: RoleRow;
  @Output() configure = new EventEmitter<RoleRow>();
}


