using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
public class FormPay : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("DG")]
	private DataGridView _DG;

	[CompilerGenerated]
	[AccessedThroughProperty("ДодатиЗасібОплатиToolStripMenuItem")]
	private ToolStripMenuItem _ДодатиЗасібОплатиToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ЗакритиToolStripMenuItem")]
	private ToolStripMenuItem _ЗакритиToolStripMenuItem;

	internal virtual DataGridView DG
	{
		[CompilerGenerated]
		get
		{
			return _DG;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			//IL_0007: Unknown result type (might be due to invalid IL or missing references)
			//IL_000d: Expected O, but got Unknown
			//IL_0014: Unknown result type (might be due to invalid IL or missing references)
			//IL_001a: Expected O, but got Unknown
			DataGridViewCellEventHandler val = new DataGridViewCellEventHandler(DG_CellDoubleClick);
			DataGridViewCellEventHandler val2 = new DataGridViewCellEventHandler(DG_CellContentClick);
			DataGridView dG = _DG;
			if (dG != null)
			{
				dG.CellDoubleClick -= val;
				dG.CellContentClick -= val2;
			}
			_DG = value;
			dG = _DG;
			if (dG != null)
			{
				dG.CellDoubleClick += val;
				dG.CellContentClick += val2;
			}
		}
	}

	[field: AccessedThroughProperty("Column1")]
	internal virtual DataGridViewTextBoxColumn Column1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column2")]
	internal virtual DataGridViewTextBoxColumn Column2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column3")]
	internal virtual DataGridViewTextBoxColumn Column3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("MenuStrip1")]
	internal virtual MenuStrip MenuStrip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("МенюToolStripMenuItem")]
	internal virtual ToolStripMenuItem МенюToolStripMenuItem
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ДодатиЗасібОплатиToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ДодатиЗасібОплатиToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ДодатиЗасібОплатиToolStripMenuItem_Click;
			ToolStripMenuItem додатиЗасібОплатиToolStripMenuItem = _ДодатиЗасібОплатиToolStripMenuItem;
			if (додатиЗасібОплатиToolStripMenuItem != null)
			{
				((ToolStripItem)додатиЗасібОплатиToolStripMenuItem).Click -= eventHandler;
			}
			_ДодатиЗасібОплатиToolStripMenuItem = value;
			додатиЗасібОплатиToolStripMenuItem = _ДодатиЗасібОплатиToolStripMenuItem;
			if (додатиЗасібОплатиToolStripMenuItem != null)
			{
				((ToolStripItem)додатиЗасібОплатиToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem1")]
	internal virtual ToolStripSeparator ToolStripMenuItem1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ЗакритиToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ЗакритиToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ЗакритиToolStripMenuItem_Click;
			ToolStripMenuItem закритиToolStripMenuItem = _ЗакритиToolStripMenuItem;
			if (закритиToolStripMenuItem != null)
			{
				((ToolStripItem)закритиToolStripMenuItem).Click -= eventHandler;
			}
			_ЗакритиToolStripMenuItem = value;
			закритиToolStripMenuItem = _ЗакритиToolStripMenuItem;
			if (закритиToolStripMenuItem != null)
			{
				((ToolStripItem)закритиToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	public FormPay()
	{
		((Form)this).Load += FormPay_Load;
		InitializeComponent();
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_0421: Unknown result type (might be due to invalid IL or missing references)
		//IL_042b: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormPay));
		DG = new DataGridView();
		Column1 = new DataGridViewTextBoxColumn();
		Column2 = new DataGridViewTextBoxColumn();
		Column3 = new DataGridViewTextBoxColumn();
		MenuStrip1 = new MenuStrip();
		МенюToolStripMenuItem = new ToolStripMenuItem();
		ДодатиЗасібОплатиToolStripMenuItem = new ToolStripMenuItem();
		ToolStripMenuItem1 = new ToolStripSeparator();
		ЗакритиToolStripMenuItem = new ToolStripMenuItem();
		((ISupportInitialize)DG).BeginInit();
		((Control)MenuStrip1).SuspendLayout();
		((Control)this).SuspendLayout();
		DG.AllowUserToAddRows = false;
		DG.AllowUserToDeleteRows = false;
		((Control)DG).Anchor = (AnchorStyles)15;
		DG.ColumnHeadersHeightSizeMode = (DataGridViewColumnHeadersHeightSizeMode)2;
		DG.Columns.AddRange((DataGridViewColumn[])(object)new DataGridViewColumn[3]
		{
			(DataGridViewColumn)Column1,
			(DataGridViewColumn)Column2,
			(DataGridViewColumn)Column3
		});
		((Control)DG).Location = new Point(-1, 31);
		((Control)DG).Name = "DG";
		DG.ReadOnly = true;
		DG.RowHeadersWidth = 51;
		DG.RowTemplate.Height = 24;
		((Control)DG).Size = new Size(1047, 453);
		((Control)DG).TabIndex = 1;
		((DataGridViewColumn)Column1).HeaderText = "№";
		((DataGridViewColumn)Column1).MinimumWidth = 6;
		((DataGridViewColumn)Column1).Name = "Column1";
		((DataGridViewColumn)Column1).ReadOnly = true;
		((DataGridViewColumn)Column1).Width = 54;
		((DataGridViewColumn)Column2).HeaderText = "Засіб оплати";
		((DataGridViewColumn)Column2).MinimumWidth = 6;
		((DataGridViewColumn)Column2).Name = "Column2";
		((DataGridViewColumn)Column2).ReadOnly = true;
		((DataGridViewColumn)Column2).Width = 450;
		((DataGridViewColumn)Column3).HeaderText = "Форма оплати";
		((DataGridViewColumn)Column3).MinimumWidth = 6;
		((DataGridViewColumn)Column3).Name = "Column3";
		((DataGridViewColumn)Column3).ReadOnly = true;
		((DataGridViewColumn)Column3).Width = 171;
		((ToolStrip)MenuStrip1).ImageScalingSize = new Size(20, 20);
		((ToolStrip)MenuStrip1).Items.AddRange((ToolStripItem[])(object)new ToolStripItem[1] { (ToolStripItem)МенюToolStripMenuItem });
		((Control)MenuStrip1).Location = new Point(0, 0);
		((Control)MenuStrip1).Name = "MenuStrip1";
		((Control)MenuStrip1).Size = new Size(1045, 28);
		((Control)MenuStrip1).TabIndex = 2;
		((Control)MenuStrip1).Text = "MenuStrip1";
		((ToolStripDropDownItem)МенюToolStripMenuItem).DropDownItems.AddRange((ToolStripItem[])(object)new ToolStripItem[3]
		{
			(ToolStripItem)ДодатиЗасібОплатиToolStripMenuItem,
			(ToolStripItem)ToolStripMenuItem1,
			(ToolStripItem)ЗакритиToolStripMenuItem
		});
		((ToolStripItem)МенюToolStripMenuItem).Name = "МенюToolStripMenuItem";
		((ToolStripItem)МенюToolStripMenuItem).Size = new Size(65, 24);
		((ToolStripItem)МенюToolStripMenuItem).Text = "Меню";
		((ToolStripItem)ДодатиЗасібОплатиToolStripMenuItem).Name = "ДодатиЗасібОплатиToolStripMenuItem";
		((ToolStripItem)ДодатиЗасібОплатиToolStripMenuItem).Size = new Size(243, 26);
		((ToolStripItem)ДодатиЗасібОплатиToolStripMenuItem).Text = "Додати засіб оплати...";
		((ToolStripItem)ToolStripMenuItem1).Name = "ToolStripMenuItem1";
		((ToolStripItem)ToolStripMenuItem1).Size = new Size(240, 6);
		((ToolStripItem)ЗакритиToolStripMenuItem).Name = "ЗакритиToolStripMenuItem";
		((ToolStripItem)ЗакритиToolStripMenuItem).Size = new Size(243, 26);
		((ToolStripItem)ЗакритиToolStripMenuItem).Text = "Закрити";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(1045, 482);
		((Control)this).Controls.Add((Control)(object)DG);
		((Control)this).Controls.Add((Control)(object)MenuStrip1);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormPay";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Засоби і форми оплати";
		((ISupportInitialize)DG).EndInit();
		((Control)MenuStrip1).ResumeLayout(false);
		((Control)MenuStrip1).PerformLayout();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void FormPay_Load(object sender, EventArgs e)
	{
		LoadPayForms();
	}

	private void LoadPayForms()
	{
		checked
		{
			try
			{
				All.PayTax.StartLoad();
				DG.RowCount = 0;
				int payN = All.PayTax.PayN;
				for (int i = 1; i <= payN; i++)
				{
					DataGridView dG;
					(dG = DG).RowCount = dG.RowCount + 1;
					DG[0, DG.RowCount - 1].Value = i.ToString();
					DG[1, DG.RowCount - 1].Value = All.PayTax.get_PayName(i).ToUpper();
					DG[2, DG.RowCount - 1].Value = All.PayTax.get_PayISCASHname(i).ToUpper();
					if (i < 5)
					{
						DG.Rows[i - 1].Cells[0].Style.BackColor = Color.FromArgb(255, 225, 225);
					}
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.Lg.SaveTextToLog("Форма Оплаты", "Ошибка заполнения таблицы с формами платежей", "Критическая ошибка");
				ProjectData.ClearProjectError();
			}
		}
	}

	private void DG_CellDoubleClick(object sender, DataGridViewCellEventArgs e)
	{
		//IL_006b: Unknown result type (might be due to invalid IL or missing references)
		string eIdPay = DG.CurrentRow.Cells[0].Value.ToString();
		string eNamePay = DG.CurrentRow.Cells[1].Value.ToString();
		string eGrPay = DG.CurrentRow.Cells[2].Value.ToString();
		((Form)new FormAddPay(eIdPay, eNamePay, eGrPay)).ShowDialog();
		LoadPayForms();
	}

	private void ЗакритиToolStripMenuItem_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void ДодатиЗасібОплатиToolStripMenuItem_Click(object sender, EventArgs e)
	{
		//IL_0014: Unknown result type (might be due to invalid IL or missing references)
		((Form)new FormAddPay()).ShowDialog();
		LoadPayForms();
	}

	private void DG_CellContentClick(object sender, DataGridViewCellEventArgs e)
	{
	}
}
