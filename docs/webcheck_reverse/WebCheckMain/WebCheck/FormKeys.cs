using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormKeys : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("DG")]
	private DataGridView _DG;

	[CompilerGenerated]
	[AccessedThroughProperty("Submit")]
	private Button _Submit;

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
			//IL_0021: Unknown result type (might be due to invalid IL or missing references)
			//IL_0027: Expected O, but got Unknown
			//IL_002e: Unknown result type (might be due to invalid IL or missing references)
			//IL_0034: Expected O, but got Unknown
			DataGridViewCellEventHandler val = new DataGridViewCellEventHandler(DG_CellContentClick);
			DataGridViewCellEventHandler val2 = new DataGridViewCellEventHandler(DG_CellClick);
			DataGridViewCellMouseEventHandler val3 = new DataGridViewCellMouseEventHandler(DG_CellMouseUp);
			DataGridViewCellEventHandler val4 = new DataGridViewCellEventHandler(DG_CellEndEdit);
			DataGridView dG = _DG;
			if (dG != null)
			{
				dG.CellContentClick -= val;
				dG.CellClick -= val2;
				dG.CellMouseUp -= val3;
				dG.CellEndEdit -= val4;
			}
			_DG = value;
			dG = _DG;
			if (dG != null)
			{
				dG.CellContentClick += val;
				dG.CellClick += val2;
				dG.CellMouseUp += val3;
				dG.CellEndEdit += val4;
			}
		}
	}

	internal virtual Button Submit
	{
		[CompilerGenerated]
		get
		{
			return _Submit;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Submit_Click;
			Button submit = _Submit;
			if (submit != null)
			{
				((Control)submit).Click -= eventHandler;
			}
			_Submit = value;
			submit = _Submit;
			if (submit != null)
			{
				((Control)submit).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("EmailT")]
	internal virtual TextBox EmailT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
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

	[field: AccessedThroughProperty("Column4")]
	internal virtual DataGridViewCheckBoxColumn Column4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column5")]
	internal virtual DataGridViewComboBoxColumn Column5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormKeys()
	{
		((Form)this).Load += FormKeys_Load;
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
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_0175: Unknown result type (might be due to invalid IL or missing references)
		//IL_017f: Expected O, but got Unknown
		//IL_01fc: Unknown result type (might be due to invalid IL or missing references)
		//IL_0206: Expected O, but got Unknown
		//IL_028b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0295: Expected O, but got Unknown
		//IL_031b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0325: Expected O, but got Unknown
		//IL_05a2: Unknown result type (might be due to invalid IL or missing references)
		//IL_05ac: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormKeys));
		DG = new DataGridView();
		Submit = new Button();
		EmailT = new TextBox();
		Label2 = new Label();
		Label1 = new Label();
		Column1 = new DataGridViewTextBoxColumn();
		Column2 = new DataGridViewTextBoxColumn();
		Column3 = new DataGridViewTextBoxColumn();
		Column4 = new DataGridViewCheckBoxColumn();
		Column5 = new DataGridViewComboBoxColumn();
		((ISupportInitialize)DG).BeginInit();
		((Control)this).SuspendLayout();
		DG.AllowUserToAddRows = false;
		DG.AllowUserToDeleteRows = false;
		DG.ColumnHeadersHeightSizeMode = (DataGridViewColumnHeadersHeightSizeMode)2;
		DG.Columns.AddRange((DataGridViewColumn[])(object)new DataGridViewColumn[5]
		{
			(DataGridViewColumn)Column1,
			(DataGridViewColumn)Column2,
			(DataGridViewColumn)Column3,
			(DataGridViewColumn)Column4,
			(DataGridViewColumn)Column5
		});
		((Control)DG).Location = new Point(5, 77);
		((Control)DG).Name = "DG";
		DG.RowHeadersWidth = 51;
		DG.RowTemplate.Height = 24;
		((Control)DG).Size = new Size(997, 469);
		((Control)DG).TabIndex = 0;
		((Control)Submit).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Submit).Location = new Point(839, 15);
		((Control)Submit).Name = "Submit";
		((Control)Submit).Size = new Size(137, 34);
		((Control)Submit).TabIndex = 1;
		((ButtonBase)Submit).Text = "Замовити";
		((ButtonBase)Submit).UseVisualStyleBackColor = true;
		((Control)EmailT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)EmailT).Location = new Point(525, 15);
		((Control)EmailT).Name = "EmailT";
		((Control)EmailT).Size = new Size(290, 30);
		((Control)EmailT).TabIndex = 2;
		EmailT.TextAlign = (HorizontalAlignment)2;
		((Control)EmailT).Visible = false;
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(446, 18);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(73, 25);
		((Control)Label2).TabIndex = 3;
		Label2.Text = "Email *";
		((Control)Label2).Visible = false;
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(12, 9);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(373, 60);
		((Control)Label1).TabIndex = 4;
		Label1.Text = "Вкажіть на які фіскальні номери Ви хочете\r\nпридбати ліцензію і на вказану адресу, \r\nми відправимо рахунок";
		((Control)Label1).Visible = false;
		((DataGridViewColumn)Column1).HeaderText = "TIN";
		((DataGridViewColumn)Column1).MinimumWidth = 6;
		((DataGridViewColumn)Column1).Name = "Column1";
		((DataGridViewColumn)Column1).ReadOnly = true;
		((DataGridViewColumn)Column1).Width = 150;
		((DataGridViewColumn)Column2).HeaderText = "FN";
		((DataGridViewColumn)Column2).MinimumWidth = 6;
		((DataGridViewColumn)Column2).Name = "Column2";
		((DataGridViewColumn)Column2).ReadOnly = true;
		((DataGridViewColumn)Column2).Width = 150;
		((DataGridViewColumn)Column3).HeaderText = "Ліцензія";
		((DataGridViewColumn)Column3).MinimumWidth = 6;
		((DataGridViewColumn)Column3).Name = "Column3";
		((DataGridViewColumn)Column3).ReadOnly = true;
		((DataGridViewColumn)Column3).Width = 125;
		((DataGridViewColumn)Column4).HeaderText = "Замовити";
		((DataGridViewColumn)Column4).MinimumWidth = 6;
		((DataGridViewColumn)Column4).Name = "Column4";
		((DataGridViewColumn)Column4).Width = 75;
		((DataGridViewColumn)Column5).HeaderText = "Термін";
		Column5.Items.AddRange(new object[3] { "12 місяців", "24 місяців", "36 місяців" });
		((DataGridViewColumn)Column5).MinimumWidth = 6;
		((DataGridViewColumn)Column5).Name = "Column5";
		((DataGridViewColumn)Column5).ReadOnly = true;
		((DataGridViewColumn)Column5).Width = 125;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(1006, 549);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)Label2);
		((Control)this).Controls.Add((Control)(object)EmailT);
		((Control)this).Controls.Add((Control)(object)Submit);
		((Control)this).Controls.Add((Control)(object)DG);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormKeys";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Ліцензія";
		((ISupportInitialize)DG).EndInit();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void FormKeys_Load(object sender, EventArgs e)
	{
		Application.DoEvents();
		new KeysUpDate().DownLoadKeysAll();
		((Form)this).AcceptButton = (IButtonControl)(object)Submit;
		int num = All.f.IndexMaxFn();
		checked
		{
			for (int i = 1; i <= num; i++)
			{
				string text = All.f.NameFn(i).Trim();
				if (text.Length == 10)
				{
					DataGridView dG;
					(dG = DG).RowCount = dG.RowCount + 1;
					string text2 = All.f.StringGetFn(text, "TIN");
					DG[0, DG.RowCount - 1].Value = text2;
					DG[1, DG.RowCount - 1].Value = text;
					DG[2, DG.RowCount - 1].Value = FullVersionT(text2, text);
				}
			}
		}
	}

	private string FullVersionT(string ttt, string fff)
	{
		KeysWC keysWC = new KeysWC();
		DateTime now = DateTime.Now;
		string text = now.Year.ToString();
		string text2 = now.Month.ToString();
		if (text2.Length < 2)
		{
			text2 = "0" + text2;
		}
		text += text2;
		string text3 = now.Day.ToString();
		if (text3.Length < 2)
		{
			text3 = "0" + text3;
		}
		text += text3;
		long num = keysWC.DhgbK(ttt, fff);
		if (num < Conversions.ToInteger(text))
		{
			return "free";
		}
		string text4 = num.ToString();
		return Conversions.ToString(text4[6]) + Conversions.ToString(text4[7]) + "/" + Conversions.ToString(text4[4]) + Conversions.ToString(text4[5]) + "/" + Conversions.ToString(text4[0]) + Conversions.ToString(text4[1]) + Conversions.ToString(text4[2]) + Conversions.ToString(text4[3]);
	}

	private void DG_CellContentClick(object sender, DataGridViewCellEventArgs e)
	{
	}

	private void DG_CellClick(object sender, DataGridViewCellEventArgs e)
	{
	}

	private void DG_CellMouseUp(object sender, DataGridViewCellMouseEventArgs e)
	{
	}

	private void DG_CellEndEdit(object sender, DataGridViewCellEventArgs e)
	{
		DG[4, ((DataGridViewBand)DG.CurrentRow).Index].ReadOnly = !Conversions.ToBoolean(DG[3, ((DataGridViewBand)DG.CurrentRow).Index].Value);
		if (DG[4, ((DataGridViewBand)DG.CurrentRow).Index].ReadOnly)
		{
			DG[4, ((DataGridViewBand)DG.CurrentRow).Index].Value = "";
		}
		else
		{
			DG[4, ((DataGridViewBand)DG.CurrentRow).Index].Value = "12 місяців";
		}
	}

	private void Submit_Click(object sender, EventArgs e)
	{
		OpenURL("https://www.webchek.com.ua/licorder/");
	}

	public void OpenURL(string wwwURL)
	{
		try
		{
			Process.Start(wwwURL);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}
}
