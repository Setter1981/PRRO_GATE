using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class Form6 : Form
{
	private IContainer components;

	[field: AccessedThroughProperty("DG")]
	internal virtual DataGridView DG
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("s1")]
	internal virtual DataGridViewTextBoxColumn s1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("s2")]
	internal virtual DataGridViewTextBoxColumn s2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("s3")]
	internal virtual DataGridViewTextBoxColumn s3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("s4")]
	internal virtual DataGridViewTextBoxColumn s4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public Form6()
	{
		((Form)this).Load += Form6_Load;
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
		//IL_0001: Unknown result type (might be due to invalid IL or missing references)
		//IL_000b: Expected O, but got Unknown
		//IL_000c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0016: Expected O, but got Unknown
		//IL_0017: Unknown result type (might be due to invalid IL or missing references)
		//IL_0021: Expected O, but got Unknown
		//IL_0022: Unknown result type (might be due to invalid IL or missing references)
		//IL_002c: Expected O, but got Unknown
		//IL_002d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0037: Expected O, but got Unknown
		DG = new DataGridView();
		s1 = new DataGridViewTextBoxColumn();
		s2 = new DataGridViewTextBoxColumn();
		s3 = new DataGridViewTextBoxColumn();
		s4 = new DataGridViewTextBoxColumn();
		((ISupportInitialize)DG).BeginInit();
		((Control)this).SuspendLayout();
		DG.AllowUserToAddRows = false;
		DG.AllowUserToDeleteRows = false;
		DG.ColumnHeadersHeightSizeMode = (DataGridViewColumnHeadersHeightSizeMode)2;
		DG.Columns.AddRange((DataGridViewColumn[])(object)new DataGridViewColumn[4]
		{
			(DataGridViewColumn)s1,
			(DataGridViewColumn)s2,
			(DataGridViewColumn)s3,
			(DataGridViewColumn)s4
		});
		((Control)DG).Location = new Point(12, 34);
		((Control)DG).Name = "DG";
		DG.ReadOnly = true;
		DG.RowHeadersWidth = 51;
		DG.RowTemplate.Height = 24;
		((Control)DG).Size = new Size(856, 497);
		((Control)DG).TabIndex = 0;
		((DataGridViewColumn)s1).HeaderText = "№";
		((DataGridViewColumn)s1).MinimumWidth = 6;
		((DataGridViewColumn)s1).Name = "s1";
		((DataGridViewColumn)s1).ReadOnly = true;
		((DataGridViewColumn)s1).Width = 75;
		((DataGridViewColumn)s2).HeaderText = "FN";
		((DataGridViewColumn)s2).MinimumWidth = 6;
		((DataGridViewColumn)s2).Name = "s2";
		((DataGridViewColumn)s2).ReadOnly = true;
		((DataGridViewColumn)s2).Width = 200;
		((DataGridViewColumn)s3).HeaderText = "Лицензия";
		((DataGridViewColumn)s3).MinimumWidth = 6;
		((DataGridViewColumn)s3).Name = "s3";
		((DataGridViewColumn)s3).ReadOnly = true;
		((DataGridViewColumn)s3).Width = 200;
		((DataGridViewColumn)s4).HeaderText = "Тариф";
		((DataGridViewColumn)s4).MinimumWidth = 6;
		((DataGridViewColumn)s4).Name = "s4";
		((DataGridViewColumn)s4).ReadOnly = true;
		((DataGridViewColumn)s4).Width = 75;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(880, 558);
		((Control)this).Controls.Add((Control)(object)DG);
		((Control)this).Name = "Form6";
		((Form)this).Text = "Form6";
		((ISupportInitialize)DG).EndInit();
		((Control)this).ResumeLayout(false);
	}

	private void Form6_Load(object sender, EventArgs e)
	{
		string tINN = WebCheck.All.l.InfaTaxObjects().tINN;
		if (Operators.CompareString(tINN.Trim(), "", false) != 0)
		{
			GetKeys(tINN);
		}
		((Form)this).Text = "TIN: " + tINN;
	}

	private void GetKeys(string tin)
	{
		WebCheck.KeysWC keysWC = new WebCheck.KeysWC();
		checked
		{
			if (keysWC.LoadKeys(tin))
			{
				int num = 0;
				string text = "";
				num = 1;
				do
				{
					text = WebCheck.All.f.GetString("Global", num.ToString(), "").Trim();
					if (text.Length == 10 && Versioned.IsNumeric((object)text))
					{
						DataGridView dG;
						(dG = DG).RowCount = dG.RowCount + 1;
						string data = keysWC.SearchKey(WebCheck.All.FN).Data;
						if (Operators.CompareString(data, "", false) != 0)
						{
							DG[0, DG.RowCount - 1].Value = num.ToString();
							DG[1, DG.RowCount - 1].Value = text;
							DG[2, DG.RowCount - 1].Value = data;
							DG[data, DG.RowCount - 1].Value = keysWC.SearchKey(WebCheck.All.FN).Tarif;
						}
						else
						{
							DG[0, DG.RowCount - 1].Value = num.ToString();
							DG[1, DG.RowCount - 1].Value = text;
							DG[2, DG.RowCount - 1].Value = "free";
						}
					}
					num++;
				}
				while (num <= 99);
			}
			_ = null;
		}
	}
}
