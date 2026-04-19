using System;
using System.ComponentModel;
using System.Data.SQLite;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class Form3 : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("ADDoperator")]
	private Button _ADDoperator;

	[field: AccessedThroughProperty("DG")]
	internal virtual DataGridView DG
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ID")]
	internal virtual DataGridViewTextBoxColumn ID
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("NAMEPAY")]
	internal virtual DataGridViewTextBoxColumn NAMEPAY
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ISCASH")]
	internal virtual DataGridViewTextBoxColumn ISCASH
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PayName")]
	internal virtual TextBox PayName
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("CashIS")]
	internal virtual TextBox CashIS
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ADDoperator
	{
		[CompilerGenerated]
		get
		{
			return _ADDoperator;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ADDoperator_Click;
			Button aDDoperator = _ADDoperator;
			if (aDDoperator != null)
			{
				((Control)aDDoperator).Click -= eventHandler;
			}
			_ADDoperator = value;
			aDDoperator = _ADDoperator;
			if (aDDoperator != null)
			{
				((Control)aDDoperator).Click += eventHandler;
			}
		}
	}

	public Form3()
	{
		((Form)this).Load += Form3_Load;
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
		//IL_01eb: Unknown result type (might be due to invalid IL or missing references)
		//IL_01f5: Expected O, but got Unknown
		//IL_026e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0278: Expected O, but got Unknown
		//IL_03c7: Unknown result type (might be due to invalid IL or missing references)
		//IL_03d1: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(Form3));
		DG = new DataGridView();
		ID = new DataGridViewTextBoxColumn();
		NAMEPAY = new DataGridViewTextBoxColumn();
		ISCASH = new DataGridViewTextBoxColumn();
		PayName = new TextBox();
		CashIS = new TextBox();
		ADDoperator = new Button();
		((ISupportInitialize)DG).BeginInit();
		((Control)this).SuspendLayout();
		DG.AllowUserToAddRows = false;
		DG.AllowUserToDeleteRows = false;
		((Control)DG).Anchor = (AnchorStyles)15;
		DG.ColumnHeadersHeightSizeMode = (DataGridViewColumnHeadersHeightSizeMode)2;
		DG.Columns.AddRange((DataGridViewColumn[])(object)new DataGridViewColumn[3]
		{
			(DataGridViewColumn)ID,
			(DataGridViewColumn)NAMEPAY,
			(DataGridViewColumn)ISCASH
		});
		((Control)DG).Location = new Point(0, 0);
		((Control)DG).Name = "DG";
		DG.ReadOnly = true;
		((Control)DG).Size = new Size(866, 331);
		((Control)DG).TabIndex = 0;
		((DataGridViewColumn)ID).HeaderText = "ID";
		((DataGridViewColumn)ID).Name = "ID";
		((DataGridViewColumn)ID).ReadOnly = true;
		((DataGridViewColumn)NAMEPAY).HeaderText = "NAMEPAY";
		((DataGridViewColumn)NAMEPAY).Name = "NAMEPAY";
		((DataGridViewColumn)NAMEPAY).ReadOnly = true;
		((DataGridViewColumn)NAMEPAY).Width = 250;
		((DataGridViewColumn)ISCASH).HeaderText = "ISCASH";
		((DataGridViewColumn)ISCASH).Name = "ISCASH";
		((DataGridViewColumn)ISCASH).ReadOnly = true;
		((DataGridViewColumn)ISCASH).Width = 250;
		((Control)PayName).Anchor = (AnchorStyles)6;
		((Control)PayName).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PayName).Location = new Point(12, 354);
		((Control)PayName).Name = "PayName";
		((Control)PayName).Size = new Size(349, 24);
		((Control)PayName).TabIndex = 1;
		PayName.TextAlign = (HorizontalAlignment)2;
		((Control)CashIS).Anchor = (AnchorStyles)6;
		((Control)CashIS).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)CashIS).Location = new Point(367, 354);
		((Control)CashIS).Name = "CashIS";
		((Control)CashIS).Size = new Size(296, 24);
		((Control)CashIS).TabIndex = 2;
		CashIS.TextAlign = (HorizontalAlignment)2;
		((Control)ADDoperator).Anchor = (AnchorStyles)6;
		((Control)ADDoperator).Location = new Point(690, 354);
		((Control)ADDoperator).Name = "ADDoperator";
		((Control)ADDoperator).Size = new Size(165, 33);
		((Control)ADDoperator).TabIndex = 6;
		((ButtonBase)ADDoperator).Text = "Добавить";
		((ButtonBase)ADDoperator).UseVisualStyleBackColor = true;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(6f, 13f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(867, 404);
		((Control)this).Controls.Add((Control)(object)ADDoperator);
		((Control)this).Controls.Add((Control)(object)CashIS);
		((Control)this).Controls.Add((Control)(object)PayName);
		((Control)this).Controls.Add((Control)(object)DG);
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Control)this).Name = "Form3";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "PayForms";
		((ISupportInitialize)DG).EndInit();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void Form3_Load(object sender, EventArgs e)
	{
		LoadOperators();
		CashIS.Text = "1";
	}

	private void LoadOperators()
	{
		checked
		{
			try
			{
				DG.RowCount = 0;
				string connectionString = "Data Source=" + WebCheck.All.FileN + "; Version=3";
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = connectionString;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "Select * FROM 'PayForms'";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				((Form)this).Text = DG.RowCount.ToString();
				while (sQLiteDataReader.Read())
				{
					DataGridView dG;
					(dG = DG).RowCount = dG.RowCount + 1;
					DG[0, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[0]);
					DG[1, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[1]);
					DG[2, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[2]);
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
				((Form)this).Text = "PayForms " + WebCheck.All.l.MaxID("PayForms");
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private void ADDoperator_Click(object sender, EventArgs e)
	{
		if (Operators.CompareString(PayName.Text.Trim(), "", false) == 0)
		{
			((Control)PayName).Focus();
			return;
		}
		if (Operators.CompareString(CashIS.Text.Trim(), "", false) == 0)
		{
			((Control)CashIS).Focus();
			return;
		}
		if (!Versioned.IsNumeric((object)CashIS.Text))
		{
			CashIS.Text = "";
			((Control)CashIS).Focus();
			return;
		}
		string connectionString = "Data Source=" + WebCheck.All.FileN + "; Version=3";
		SQLiteConnection sQLiteConnection = new SQLiteConnection();
		SQLiteCommand sQLiteCommand = new SQLiteCommand();
		sQLiteConnection.ConnectionString = connectionString;
		sQLiteConnection.Open();
		sQLiteCommand = sQLiteConnection.CreateCommand();
		sQLiteCommand.CommandText = "INSERT INTO 'PayForms' (NAME, ISCASH) VALUES ('" + PayName.Text + "','" + CashIS.Text + "')";
		SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
		((Component)(object)sQLiteCommand).Dispose();
		sQLiteDataReader.Close();
		sQLiteConnection.Close();
		PayName.Text = "";
		CashIS.Text = "1";
		LoadOperators();
	}
}
